//! SDL3 platform backend: window management, High-DPI, event translation,
//! GPU-composited rendering via `SDL_Renderer`, and the desktop HUD.

use std::time::{Duration, Instant};

use bevy_app::App;
use bevy_ecs::prelude::World;
use bevy_state::prelude::State;
use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use sdl3::mouse::MouseButton;
use sdl3::pixels::{Color, PixelFormat};
use sdl3::render::{BlendMode, Canvas, FPoint, FRect, ScaleMode, Texture, TextureCreator};
use sdl3::video::{Window, WindowContext};

use crate::app::{AppRenderer, AppScreen, app_screen, has_mickey_cursor, should_exit, step_app};
use crate::ecs::components::{
    AnimationState, HintHighlight, PieceGeometry, PieceIdentity, PieceLocation,
};
use crate::ecs::resources::{
    ActivePuzzle, AudioStatus, GameConfig, InGameMenuState, RenderVersions, ScreenState,
    StaticAssets,
};
use crate::graphics::{SCREEN_H, SCREEN_W};
use crate::input::{
    InputDeviceKind, InputKey, InputRouter, PlatformEvent, PointerButton, ViewportTransform,
};
use crate::puzzle::ToolboxTarget;

const SDL_TOUCH_MOUSEID: u32 = sdl3::sys::touch::SDL_TOUCH_MOUSEID.0;
const FALLBACK_FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);

fn compute_frame_sleep(
    frame_elapsed: Duration,
    target_interval: Duration,
    vsync_enabled: bool,
) -> Option<Duration> {
    if vsync_enabled || frame_elapsed >= target_interval {
        None
    } else {
        Some(target_interval - frame_elapsed)
    }
}

type SdlResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, PartialEq, Eq)]
struct BoardTexKey {
    session_id: u64,
    asset_rev: u64,
    board_rev: u64,
    picture_guide: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ToolboxTexKey {
    session_id: u64,
    asset_rev: u64,
    toolbox_open: bool,
    tray_rev: u64,
    hovered: Option<ToolboxTarget>,
    pressed: Option<ToolboxTarget>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HeldTexKey {
    session_id: u64,
    asset_rev: u64,
    board_rev: u64,
    held_piece: usize,
    anchor_cell: Option<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HintTexKey {
    session_id: u64,
    asset_rev: u64,
    hint_idx: usize,
    playfield_x: i32,
}

#[derive(Clone, PartialEq, Eq)]
struct OverlayTexKey {
    status: Option<String>,
    caption: Option<String>,
    has_error: bool,
}

struct CachedLayer<'a> {
    tex: Texture<'a>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

struct SdlGameRenderer<'a> {
    creator: &'a TextureCreator<WindowContext>,
    base_tex: Texture<'a>,
    ref_tex: Texture<'a>,
    cached_ref_key: Option<(u64, u64)>,
    board_tex_key: Option<BoardTexKey>,
    last_rendered_interactive: bool,
    toolbox_layer: Option<(ToolboxTexKey, CachedLayer<'a>)>,
    held_layer: Option<(HeldTexKey, CachedLayer<'a>)>,
    hint_layer: Option<(HintTexKey, CachedLayer<'a>)>,
    overlay_tex: Option<(OverlayTexKey, Texture<'a>)>,
    cursor_texs: [Option<(Texture<'a>, i32, i32)>; 3],
    scratch_rgba: Vec<u8>,
    board_rgba: Vec<u8>,
    overlay_rgba: Vec<u8>,
}

impl<'a> SdlGameRenderer<'a> {
    fn new(creator: &'a TextureCreator<WindowContext>, world: &World) -> SdlResult<Self> {
        let mut base_tex = creator.create_texture_streaming(
            PixelFormat::RGBA32,
            SCREEN_W as u32,
            SCREEN_H as u32,
        )?;
        base_tex.set_scale_mode(ScaleMode::Nearest);
        base_tex.set_blend_mode(BlendMode::None);

        let mut ref_tex = creator.create_texture_streaming(
            PixelFormat::RGBA32,
            SCREEN_W as u32,
            SCREEN_H as u32,
        )?;
        ref_tex.set_scale_mode(ScaleMode::Nearest);
        ref_tex.set_blend_mode(BlendMode::None);

        let mut scratch_rgba = Vec::new();
        let mut cursor_texs = [None, None, None];
        for (slot, sprite_idx) in [62usize, 63, 64].into_iter().enumerate() {
            if let Some((w, h)) =
                AppRenderer::render_cursor_rgba(world, sprite_idx, &mut scratch_rgba)
            {
                let mut tex =
                    creator.create_texture_static(PixelFormat::RGBA32, w as u32, h as u32)?;
                tex.set_scale_mode(ScaleMode::Nearest);
                tex.set_blend_mode(BlendMode::Blend);
                tex.update(None, &scratch_rgba, (w * 4) as usize)?;
                cursor_texs[slot] = Some((tex, w, h));
            }
        }

        let buf_len = (SCREEN_W * SCREEN_H * 4) as usize;
        Ok(Self {
            creator,
            base_tex,
            ref_tex,
            cached_ref_key: None,
            board_tex_key: None,
            last_rendered_interactive: false,
            toolbox_layer: None,
            held_layer: None,
            hint_layer: None,
            overlay_tex: None,
            cursor_texs,
            scratch_rgba,
            board_rgba: vec![0u8; buf_len],
            overlay_rgba: vec![0u8; buf_len],
        })
    }

    fn dos_rect_to_viewport(tf: &ViewportTransform, dx: i32, dy: i32, dw: i32, dh: i32) -> FRect {
        FRect::new(
            tf.viewport.x + dx as f32 * tf.scale,
            tf.viewport.y + dy as f32 * tf.scale,
            dw as f32 * tf.scale,
            dh as f32 * tf.scale,
        )
    }

    fn upload_rgba_layer(
        creator: &'a TextureCreator<WindowContext>,
        w: i32,
        h: i32,
        rgba: &[u8],
        blend: BlendMode,
    ) -> SdlResult<Texture<'a>> {
        let mut tex = creator.create_texture_static(PixelFormat::RGBA32, w as u32, h as u32)?;
        tex.set_scale_mode(ScaleMode::Nearest);
        tex.set_blend_mode(blend);
        tex.update(None, rgba, (w * 4) as usize)?;
        Ok(tex)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame(
        &mut self,
        canvas: &mut Canvas<Window>,
        world: &mut World,
        app_renderer: &mut AppRenderer,
        tf: &ViewportTransform,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
        hud_ref_hovered: bool,
        active_device: InputDeviceKind,
    ) -> SdlResult<()> {
        canvas.set_scale(
            (canvas
                .output_size()
                .map(|(w, _)| w as f32)
                .unwrap_or(tf.win_w)
                / tf.win_w)
                .max(0.5),
            (canvas
                .output_size()
                .map(|(_, h)| h as f32)
                .unwrap_or(tf.win_h)
                / tf.win_h)
                .max(0.5),
        )?;

        canvas.set_draw_color(Color::RGB(0x26, 0x36, 0x40));
        canvas.clear();

        let screen = *world.resource::<State<AppScreen>>().get();
        let in_game_menu_open = world.resource::<InGameMenuState>().open;
        let puzzle_solved = world
            .resource::<ActivePuzzle>()
            .0
            .as_ref()
            .is_some_and(|p| p.solved);
        let has_active_anim = world
            .query::<&AnimationState>()
            .iter(world)
            .next()
            .is_some();
        let interactive_board = screen == AppScreen::Playing
            && !show_reference
            && !in_game_menu_open
            && world.resource::<ActivePuzzle>().0.is_some()
            && !puzzle_solved
            && !has_active_anim;

        if interactive_board {
            self.render_interactive_board(canvas, world, app_renderer, tf, mx, my, picture_guide)?;
            self.last_rendered_interactive = true;
        } else {
            let uploaded =
                app_renderer.render_base_only(world, mx, my, show_reference, picture_guide);
            if uploaded || self.last_rendered_interactive {
                self.base_tex
                    .update(None, app_renderer.base_rgba(), (SCREEN_W * 4) as usize)?;
            }
            self.last_rendered_interactive = false;
            // Invalidate board_tex_key so returning from menu/reference re-syncs cleanly
            self.board_tex_key = None;
            let dst = FRect::new(tf.viewport.x, tf.viewport.y, tf.viewport.w, tf.viewport.h);
            canvas.copy(&self.base_tex, None, Some(dst))?;
        }

        // Draw Mickey cursor on GPU (hidden for touch modality so it doesn't float stale)
        if active_device != InputDeviceKind::Touch
            && let Some((sprite_idx, px, py)) =
                AppRenderer::desired_cursor(world, mx, my, show_reference)
            && let Some(slot) = sprite_idx.checked_sub(62)
            && let Some(Some((tex, cw, ch))) = self.cursor_texs.get(slot)
        {
            let dst = Self::dos_rect_to_viewport(tf, px, py, *cw, *ch);
            canvas.copy(tex, None, Some(dst))?;
        }

        // Desktop HUD
        if tf.hud_visible(screen == AppScreen::Playing)
            && world.resource::<ActivePuzzle>().0.is_some()
        {
            self.render_desktop_hud(
                canvas,
                world,
                tf,
                (hud_ref_hovered, show_reference, active_device),
            )?;
        } else if screen == AppScreen::Playing
            && let Some(active) = world
                .resource::<ActivePuzzle>()
                .0
                .as_ref()
                .filter(|p| p.timer_enabled)
        {
            let time = active.formatted_time();
            draw_scaled_text(
                canvas,
                &time,
                tf.win_w - (time.len() as f32 * 16.0) - 12.0,
                8.0,
                2.0,
                Color::RGB(255, 255, 255),
            )?;
        }

        canvas.present();
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_interactive_board(
        &mut self,
        canvas: &mut Canvas<Window>,
        world: &mut World,
        app_renderer: &mut AppRenderer,
        tf: &ViewportTransform,
        mx: i32,
        my: i32,
        picture_guide: bool,
    ) -> SdlResult<()> {
        let versions = *world.resource::<RenderVersions>();
        let (session_id, asset_rev, uses_ega, (px, py)) = {
            let meta = world.resource::<ActivePuzzle>().0.as_ref().unwrap();
            (
                meta.session_id,
                meta.asset_rev,
                meta.asset.uses_ega(),
                meta.active_pointer(mx, my),
            )
        };

        // 1. Static board layer (with picture guide baked in once per board_rev/guide toggle)
        let board_key = BoardTexKey {
            session_id,
            asset_rev,
            board_rev: versions.board_rev,
            picture_guide,
        };
        if self.board_tex_key != Some(board_key) {
            world.resource_scope(|world, assets: bevy_ecs::prelude::Mut<StaticAssets>| {
                let tbx = if uses_ega {
                    &assets.tbx_ega
                } else {
                    &assets.tbx
                };
                app_renderer.puzzle_renderer.render_board_layer_rgba(
                    world,
                    tbx,
                    picture_guide,
                    &mut self.board_rgba,
                );
            });
            self.base_tex
                .update(None, &self.board_rgba, (SCREEN_W * 4) as usize)?;
            self.board_tex_key = Some(board_key);
        }
        let board_dst = FRect::new(tf.viewport.x, tf.viewport.y, tf.viewport.w, tf.viewport.h);
        canvas.copy(&self.base_tex, None, Some(board_dst))?;

        // 2. F1 hint silhouette (tinted on GPU without re-uploading)
        let hint_info = world
            .query::<(&PieceIdentity, &HintHighlight)>()
            .iter(world)
            .next()
            .map(|(id, h)| (id.index, h.timer.remaining_secs()));
        if let Some((hidx, rem)) = hint_info {
            let playfield_x = world
                .resource::<ActivePuzzle>()
                .0
                .as_ref()
                .map_or(0, |m| m.playfield_x());
            let hint_key = HintTexKey {
                session_id,
                asset_rev,
                hint_idx: hidx,
                playfield_x,
            };
            let rebuild = self.hint_layer.as_ref().is_none_or(|(k, _)| *k != hint_key);
            if rebuild {
                if let Some((hx, hy, hw, hh)) = app_renderer
                    .puzzle_renderer
                    .render_hint_layer_rgba(world, &mut self.scratch_rgba)
                {
                    let tex = Self::upload_rgba_layer(
                        self.creator,
                        hw,
                        hh,
                        &self.scratch_rgba,
                        BlendMode::Blend,
                    )?;
                    self.hint_layer = Some((
                        hint_key,
                        CachedLayer {
                            tex,
                            x: hx,
                            y: hy,
                            w: hw,
                            h: hh,
                        },
                    ));
                } else {
                    self.hint_layer = None;
                }
            }
            if let Some((_, layer)) = &mut self.hint_layer {
                let blink = ((rem * 8.0) as i32) & 1 == 0;
                if blink {
                    layer.tex.set_color_mod(255, 255, 255);
                } else {
                    layer.tex.set_color_mod(255, 0, 0);
                }
                let dst = Self::dos_rect_to_viewport(tf, layer.x, layer.y, layer.w, layer.h);
                canvas.copy(&layer.tex, None, Some(dst))?;
            }
        } else {
            self.hint_layer = None;
        }

        // 3. Toolbox strip or closed tab
        let tray_len = world
            .query::<&PieceLocation>()
            .iter(world)
            .filter(|loc| matches!(loc, PieceLocation::Tray { .. }))
            .count();
        let (toolbox_open, pressed, hovered) = {
            let meta = world.resource::<ActivePuzzle>().0.as_ref().unwrap();
            (
                meta.toolbox_open,
                meta.pressed_toolbox_target,
                meta.toolbox_target_at(px, py, false, tray_len),
            )
        };
        let tbx_key = ToolboxTexKey {
            session_id,
            asset_rev,
            toolbox_open,
            tray_rev: versions.tray_rev,
            hovered,
            pressed,
        };
        if self
            .toolbox_layer
            .as_ref()
            .is_none_or(|(k, _)| *k != tbx_key)
        {
            let (tx, ty, tw, th) =
                world.resource_scope(|world, assets: bevy_ecs::prelude::Mut<StaticAssets>| {
                    let tbx = if uses_ega {
                        &assets.tbx_ega
                    } else {
                        &assets.tbx
                    };
                    app_renderer.puzzle_renderer.render_toolbox_layer_rgba(
                        world,
                        tbx,
                        &assets.font,
                        px,
                        py,
                        &mut self.scratch_rgba,
                    )
                });
            let tex =
                Self::upload_rgba_layer(self.creator, tw, th, &self.scratch_rgba, BlendMode::None)?;
            self.toolbox_layer = Some((
                tbx_key,
                CachedLayer {
                    tex,
                    x: tx,
                    y: ty,
                    w: tw,
                    h: th,
                },
            ));
        }
        if let Some((_, layer)) = &self.toolbox_layer {
            let dst = Self::dos_rect_to_viewport(tf, layer.x, layer.y, layer.w, layer.h);
            canvas.copy(&layer.tex, None, Some(dst))?;
        }

        // 4. Held piece / connected group (uploaded once per pickup/cell; zero CPU work while dragging)
        let mut held_anchor: Option<(usize, i32, i32)> = None;
        let mut held_items: Vec<(usize, bool, (i32, i32))> = Vec::new();
        for (ident, geom, loc) in world
            .query::<(&PieceIdentity, &PieceGeometry, &PieceLocation)>()
            .iter(world)
        {
            if let PieceLocation::Held {
                is_anchor,
                group_offset,
                ..
            } = *loc
            {
                held_items.push((ident.index, is_anchor, group_offset));
                if is_anchor {
                    held_anchor = Some((ident.index, geom.base_w, geom.base_h));
                }
            }
        }
        if let Some((pidx, anchor_w, anchor_h)) = held_anchor {
            let anchor_cell = {
                let meta = world.resource::<ActivePuzzle>().0.as_ref().unwrap();
                crate::ecs::puzzle_ops::held_group_anchor_cell(
                    meta,
                    px,
                    py,
                    held_items.iter().copied(),
                )
            };
            let held_key = HeldTexKey {
                session_id,
                asset_rev,
                board_rev: versions.board_rev,
                held_piece: pidx,
                anchor_cell,
            };
            if self.held_layer.as_ref().is_none_or(|(k, _)| *k != held_key) {
                if let Some((rx, ry, hw, hh)) = app_renderer
                    .puzzle_renderer
                    .render_held_group_layer_rgba(world, px, py, &mut self.scratch_rgba)
                {
                    let tex = Self::upload_rgba_layer(
                        self.creator,
                        hw,
                        hh,
                        &self.scratch_rgba,
                        BlendMode::Blend,
                    )?;
                    self.held_layer = Some((
                        held_key,
                        CachedLayer {
                            tex,
                            x: rx,
                            y: ry,
                            w: hw,
                            h: hh,
                        },
                    ));
                } else {
                    self.held_layer = None;
                }
            }
            if let Some((_, layer)) = &self.held_layer {
                let ax = px - anchor_w / 2;
                let ay = py - anchor_h / 2;
                let dst =
                    Self::dos_rect_to_viewport(tf, ax + layer.x, ay + layer.y, layer.w, layer.h);
                canvas.copy(&layer.tex, None, Some(dst))?;
            }
        } else {
            self.held_layer = None;
        }

        // 5. Status / caption / error overlay
        let overlay_key = OverlayTexKey {
            status: world
                .resource::<ActivePuzzle>()
                .0
                .as_ref()
                .and_then(|m| m.status_message())
                .map(str::to_owned),
            caption: (world.resource::<GameConfig>().prefs.text_msg_on != 0)
                .then(|| world.resource::<AudioStatus>().common_caption.clone())
                .flatten(),
            has_error: world.resource::<ScreenState>().error.is_some(),
        };
        if overlay_key.status.is_some() || overlay_key.caption.is_some() || overlay_key.has_error {
            if self
                .overlay_tex
                .as_ref()
                .is_none_or(|(k, _)| *k != overlay_key)
            {
                if app_renderer.render_playing_overlay_rgba(world, &mut self.overlay_rgba) {
                    let tex = Self::upload_rgba_layer(
                        self.creator,
                        SCREEN_W,
                        SCREEN_H,
                        &self.overlay_rgba,
                        BlendMode::Blend,
                    )?;
                    self.overlay_tex = Some((overlay_key, tex));
                } else {
                    self.overlay_tex = None;
                }
            }
            if let Some((_, tex)) = &self.overlay_tex {
                canvas.copy(tex, None, Some(board_dst))?;
            }
        } else {
            self.overlay_tex = None;
        }

        // 6. Keyboard pointer crosshair
        if let Some((kx, ky)) = world
            .resource::<ActivePuzzle>()
            .0
            .as_ref()
            .and_then(|m| m.keyboard_pointer)
        {
            let (kx, ky) = (kx.round() as i32, ky.round() as i32);
            let outer = Self::dos_rect_to_viewport(tf, kx - 3, ky - 3, 7, 7);
            canvas.set_draw_color(Color::RGB(0, 0, 0));
            canvas.draw_rect(outer)?;
            let hbar = Self::dos_rect_to_viewport(tf, kx - 2, ky, 5, 1);
            let vbar = Self::dos_rect_to_viewport(tf, kx, ky - 2, 1, 5);
            canvas.set_draw_color(Color::RGB(255, 255, 255));
            canvas.fill_rect(hbar)?;
            canvas.fill_rect(vbar)?;
        }

        Ok(())
    }

    fn render_desktop_hud(
        &mut self,
        canvas: &mut Canvas<Window>,
        world: &mut bevy_ecs::prelude::World,
        tf: &ViewportTransform,
        (reference_hovered, show_reference, active_device): (bool, bool, InputDeviceKind),
    ) -> SdlResult<()> {
        let win_w = tf.win_w;
        let win_h = tf.win_h;
        let ink = Color::RGB(0xf5, 0xed, 0xd8);
        let muted = Color::RGB(0xb2, 0xc4, 0xca);
        let accent = Color::RGB(0xed, 0xbe, 0x5e);
        let bar_bg = Color::RGB(0x18, 0x28, 0x31);

        canvas.set_draw_color(bar_bg);
        canvas.fill_rect(FRect::new(0.0, 0.0, win_w, 80.0))?;

        let (session_id, asset_rev) = {
            let meta = world.resource::<ActivePuzzle>().0.as_ref().unwrap();
            (meta.session_id, meta.asset_rev)
        };
        let ref_key = (session_id, asset_rev);
        if self.cached_ref_key != Some(ref_key) {
            world
                .resource::<ActivePuzzle>()
                .0
                .as_ref()
                .unwrap()
                .full_picture_fb
                .to_rgba8(&mut self.scratch_rgba);
            self.ref_tex
                .update(None, &self.scratch_rgba, (SCREEN_W * 4) as usize)?;
            self.cached_ref_key = Some(ref_key);
        }
        canvas.copy(
            &self.ref_tex,
            Some(FRect::new(2.0, 2.0, 270.0, 196.0)),
            Some(FRect::new(16.0, 10.0, 83.0, 60.0)),
        )?;

        let compact = win_w < 720.0;
        let title = if compact {
            "Mickey's Puzzles"
        } else {
            "Mickey's Jigsaw Puzzles"
        };
        draw_scaled_text(
            canvas,
            title,
            112.0,
            14.0,
            if compact { 1.75 } else { 2.0 },
            ink,
        )?;

        let mut total_pieces = 0usize;
        let mut placed = 0usize;
        let mut is_holding = false;
        for loc in world.query::<&PieceLocation>().iter(world) {
            total_pieces += 1;
            match loc {
                PieceLocation::Board { .. } => placed += 1,
                PieceLocation::Held { .. } => is_holding = true,
                PieceLocation::Tray { .. } => {}
            }
        }
        let meta = world.resource::<ActivePuzzle>().0.as_ref().unwrap();
        let puzzle_num = meta.puzzle_num;
        let timer_enabled = meta.timer_enabled;
        let formatted_time = meta.formatted_time();
        let solved = meta.solved;

        let status = if compact {
            format!("{puzzle_num:02} / {placed} of {total_pieces}")
        } else {
            format!("Picture {puzzle_num:02}  /  {placed} of {total_pieces} pieces placed")
        };
        draw_scaled_text(canvas, &status, 112.0, 44.0, 1.5, muted)?;

        if timer_enabled {
            let text_w = formatted_time.len() as f32 * 8.0 * 2.0;
            draw_scaled_text(
                canvas,
                &formatted_time,
                win_w - text_w - 20.0,
                12.0,
                2.0,
                ink,
            )?;
        }

        let btn = tf.reference_button_rect();
        let btn_col = if show_reference {
            accent
        } else if reference_hovered && !solved {
            Color::RGB(0x49, 0x66, 0x75)
        } else {
            Color::RGB(0x36, 0x4c, 0x57)
        };
        canvas.set_draw_color(btn_col);
        canvas.fill_rect(FRect::new(btn.x, btn.y, btn.w, btn.h))?;

        let btn_label = if solved {
            "Picture complete!"
        } else if show_reference {
            "Release to play"
        } else {
            "Hold: picture [Tab]"
        };
        draw_scaled_text(
            canvas,
            btn_label,
            btn.x + 12.0,
            btn.y + 9.0,
            1.25,
            if show_reference { bar_bg } else { ink },
        )?;

        canvas.set_draw_color(bar_bg);
        canvas.fill_rect(FRect::new(0.0, win_h - 36.0, win_w, 36.0))?;
        let help = if compact {
            "Tab: picture  H: guide  F1: hint  Esc: menu"
        } else if show_reference {
            "Picture reference  /  progress and timer paused"
        } else if solved {
            "Tap/click picture to play  /  Esc: choose another puzzle"
        } else if active_device == InputDeviceKind::Touch {
            if is_holding {
                "Drag to board  /  Tap tray to return piece"
            } else {
                "Tap/drag pieces  /  Long-press or 2-finger tap: toolbox/group  /  2-finger drag: scroll"
            }
        } else if is_holding {
            "Place on the board  /  Click the tray to return the piece"
        } else {
            "Pick and place  /  H: guide  G: edges  F1: hint  Esc: menu"
        };
        draw_scaled_text(canvas, help, 16.0, win_h - 24.0, 1.35, muted)?;

        Ok(())
    }
}

fn draw_scaled_text(
    canvas: &mut Canvas<Window>,
    text: &str,
    x: f32,
    y: f32,
    text_scale: f32,
    color: Color,
) -> SdlResult<()> {
    let (prev_sx, prev_sy) = canvas.scale();
    canvas.set_scale(prev_sx * text_scale, prev_sy * text_scale)?;
    canvas.set_draw_color(color);
    let res = canvas.draw_debug_text(text, FPoint::new(x / text_scale, y / text_scale));
    canvas.set_scale(prev_sx, prev_sy)?;
    res?;
    Ok(())
}

fn translate_keycode(kc: Keycode) -> Option<InputKey> {
    Some(match kc {
        Keycode::Up => InputKey::Up,
        Keycode::Down => InputKey::Down,
        Keycode::Left => InputKey::Left,
        Keycode::Right => InputKey::Right,
        Keycode::Return | Keycode::KpEnter => InputKey::Enter,
        Keycode::Space => InputKey::Space,
        Keycode::Escape => InputKey::Escape,
        Keycode::Tab => InputKey::Tab,
        Keycode::W => InputKey::W,
        Keycode::A => InputKey::A,
        Keycode::S => InputKey::S,
        Keycode::D => InputKey::D,
        Keycode::E => InputKey::E,
        Keycode::R => InputKey::R,
        Keycode::G => InputKey::G,
        Keycode::P => InputKey::P,
        Keycode::Q => InputKey::Q,
        Keycode::T => InputKey::T,
        Keycode::M => InputKey::M,
        Keycode::H => InputKey::H,
        Keycode::F1 => InputKey::F1,
        Keycode::F2 => InputKey::F2,
        Keycode::F3 => InputKey::F3,
        Keycode::F4 => InputKey::F4,
        Keycode::F5 => InputKey::F5,
        Keycode::F7 => InputKey::F7,
        Keycode::F8 => InputKey::F8,
        _ => return None,
    })
}

fn translate_sdl_event(ev: Event, win_w: f32, win_h: f32) -> Option<PlatformEvent> {
    match ev {
        Event::MouseMotion { x, y, which, .. } => Some(PlatformEvent::MouseMove {
            x,
            y,
            is_touch_synth: which == SDL_TOUCH_MOUSEID,
        }),
        Event::MouseButtonDown {
            mouse_btn,
            x,
            y,
            which,
            ..
        } => {
            let button = match mouse_btn {
                MouseButton::Left => PointerButton::Left,
                MouseButton::Right => PointerButton::Right,
                _ => return None,
            };
            Some(PlatformEvent::MouseDown {
                button,
                x,
                y,
                is_touch_synth: which == SDL_TOUCH_MOUSEID,
            })
        }
        Event::MouseButtonUp {
            mouse_btn,
            x,
            y,
            which,
            ..
        } => {
            let button = match mouse_btn {
                MouseButton::Left => PointerButton::Left,
                MouseButton::Right => PointerButton::Right,
                _ => return None,
            };
            Some(PlatformEvent::MouseUp {
                button,
                x,
                y,
                is_touch_synth: which == SDL_TOUCH_MOUSEID,
            })
        }
        Event::MouseWheel { y, which, .. } => Some(PlatformEvent::MouseWheel {
            delta_y: y,
            is_touch_synth: which == SDL_TOUCH_MOUSEID,
        }),
        Event::FingerDown {
            finger_id, x, y, ..
        } => Some(PlatformEvent::TouchDown {
            finger_id,
            x: x * win_w,
            y: y * win_h,
        }),
        Event::FingerMotion {
            finger_id, x, y, ..
        } => Some(PlatformEvent::TouchMove {
            finger_id,
            x: x * win_w,
            y: y * win_h,
        }),
        Event::FingerUp {
            finger_id, x, y, ..
        } => Some(PlatformEvent::TouchUp {
            finger_id,
            x: x * win_w,
            y: y * win_h,
        }),
        Event::FingerCanceled { finger_id, .. } => Some(PlatformEvent::TouchCancel { finger_id }),
        Event::KeyDown {
            keycode: Some(kc),
            repeat,
            ..
        } => translate_keycode(kc).map(|key| PlatformEvent::KeyDown { key, repeat }),
        Event::KeyUp {
            keycode: Some(kc), ..
        } => translate_keycode(kc).map(|key| PlatformEvent::KeyUp { key }),
        Event::Window {
            win_event: sdl3::event::WindowEvent::FocusLost,
            ..
        } => Some(PlatformEvent::WindowFocusLost),
        _ => None,
    }
}

pub fn run(mut app: App) -> Result<(), String> {
    // Disable synthetic mouse<->touch translation so touch & mouse remain cleanly separated.
    sdl3::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");
    sdl3::hint::set("SDL_MOUSE_TOUCH_EVENTS", "0");

    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let video = sdl.video().map_err(|e| e.to_string())?;
    let window = video
        .window("Mickey's Jigsaw Puzzles (1991 DOS Recreation)", 960, 720)
        .resizable()
        .high_pixel_density()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas();
    let vsync_enabled = unsafe { sdl3::sys::render::SDL_SetRenderVSync(canvas.raw(), 1) };

    let creator = canvas.texture_creator();
    let mut sdl_renderer =
        SdlGameRenderer::new(&creator, app.world()).map_err(|e| e.to_string())?;
    let mut app_renderer = AppRenderer::new();
    let mut input_router = InputRouter::new();
    let mut event_pump = sdl.event_pump().map_err(|e| e.to_string())?;

    let mut picture_guide = true;
    let mut cursor_visible = true;
    let mut last_frame = Instant::now();

    'main: loop {
        let now = Instant::now();
        let dt = now
            .duration_since(last_frame)
            .as_secs_f32()
            .clamp(0.0, 0.25);
        last_frame = now;

        let (win_w_u, win_h_u) = canvas.window().size();
        let win_w = win_w_u.max(1) as f32;
        let win_h = win_h_u.max(1) as f32;
        let pixel_density = canvas.window().pixel_density();

        for ev in event_pump.poll_iter() {
            if matches!(ev, Event::Quit { .. }) {
                break 'main;
            }
            if let Some(pev) = translate_sdl_event(ev, win_w, win_h) {
                input_router.push_event(pev);
            }
        }

        let playing = app_screen(&app) == AppScreen::Playing;
        let transform = ViewportTransform::new(win_w, win_h, pixel_density, playing);
        let reference_available = playing
            && app
                .world()
                .resource::<ActivePuzzle>()
                .0
                .as_ref()
                .is_some_and(|p| !p.solved);
        let frame = input_router.finish_frame(dt, &transform, playing, reference_available);

        if frame.toggle_picture_guide {
            picture_guide = !picture_guide;
        }

        if frame.show_reference {
            crate::ecs::systems::puzzle_input::cancel_pointer_drag(app.world_mut());
        } else {
            step_app(&mut app, &frame.app_input);
            if should_exit(&app) {
                break 'main;
            }
        }

        let mx = frame.app_input.pointer_x;
        let my = frame.app_input.pointer_y;
        let inside = (0..SCREEN_W).contains(&mx) && (0..SCREEN_H).contains(&my);
        let kbd_active = app_screen(&app) == AppScreen::Playing
            && app
                .world()
                .resource::<ActivePuzzle>()
                .0
                .as_ref()
                .is_some_and(|p| p.keyboard_pointer.is_some());
        let want_system_cursor = !(inside
            && !frame.show_reference
            && !kbd_active
            && frame.active_device != InputDeviceKind::Touch
            && has_mickey_cursor(&app));
        if want_system_cursor != cursor_visible {
            sdl.mouse().show_cursor(want_system_cursor);
            cursor_visible = want_system_cursor;
        }

        let post_playing = app_screen(&app) == AppScreen::Playing;
        let post_transform = if post_playing == playing {
            transform
        } else {
            ViewportTransform::new(win_w, win_h, pixel_density, post_playing)
        };

        sdl_renderer
            .render_frame(
                &mut canvas,
                app.world_mut(),
                &mut app_renderer,
                &post_transform,
                mx,
                my,
                frame.show_reference,
                picture_guide,
                frame.hud_ref_hovered,
                frame.active_device,
            )
            .map_err(|e| e.to_string())?;

        // Enforce a real minimum frame interval when vsync is unavailable
        if let Some(sleep_dur) =
            compute_frame_sleep(last_frame.elapsed(), FALLBACK_FRAME_INTERVAL, vsync_enabled)
        {
            std::thread::sleep(sleep_dur);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_pacing_enforces_target_interval_without_vsync() {
        let target = FALLBACK_FRAME_INTERVAL;
        assert_eq!(
            compute_frame_sleep(Duration::from_millis(2), target, true),
            None
        );
        assert_eq!(
            compute_frame_sleep(Duration::from_millis(2), target, false),
            Some(target - Duration::from_millis(2))
        );
        assert_eq!(compute_frame_sleep(target, target, false), None);
        assert_eq!(
            compute_frame_sleep(target + Duration::from_millis(3), target, false),
            None
        );
    }
}
