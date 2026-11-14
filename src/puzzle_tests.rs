use std::path::{Path, PathBuf};

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use bevy_state::prelude::State;

use super::*;
use crate::app::{AppInput, AppScreen, step_app};
use crate::assets::{
    FontAsset, FrameCommand, Preferences, PuzzleAsset, SceneRecord, SequencePlayback, Sprite8061,
    ToolboxAsset,
};
use crate::audio::AudioEngine;
use crate::ecs::components::{
    AnimationState, HintHighlight, PieceGeometry, PieceIdentity, PieceLocation, PieceSprite,
    SceneHotspot,
};
use crate::ecs::events::AudioCommand;
use crate::ecs::plugins::{
    AudioSyncPlugin, BrowserPlugin, CoreGamePlugin, PuzzlePlugin, RenderTrackingPlugin,
    SetupPlugin, sync_audio_status,
};
use crate::ecs::puzzle_ops::{
    apply_frame_side_effects, build_animation_state, complete_puzzle, flush_audio_commands,
    solved_sequence_at,
};
use crate::ecs::resources::{
    ActivePuzzle, AudioStatus, BrowserState, GameConfig, InGameMenuState, PuzzleMeta,
    RenderVersions, SetupState, StaticAssets,
};
use crate::ecs::systems::browser::spawn_puzzle_session;
use crate::ecs::systems::menu::move_keyboard_pointer_meta;
use crate::ecs::systems::puzzle_input::cancel_pointer_drag;
use crate::ecs::systems::setup::replace_scene_asset_ecs;
use crate::graphics::{
    Framebuffer, SCENE_OFFSET_X, render_scene_frame, render_scene_view,
};
use crate::puzzle_render::{PuzzleRenderer, puzzle_present_key};
use crate::setup::SetupMenu;

struct EcsPuzzleHarness {
    app: App,
}

impl EcsPuzzleHarness {
    fn new(
        puzzle_num: usize,
        asset: PuzzleAsset,
        grid_dim: usize,
        curved_edges: bool,
        grid_lines: bool,
        timer_enabled: bool,
        audio: AudioEngine,
    ) -> Self {
        let prefs = Preferences::default();
        let tbx = ToolboxAsset {
            palette: [[0; 3]; 256],
            sprites: Vec::new(),
        };
        let tbx_ega = tbx.clone();
        let font = FontAsset {
            name: String::new(),
            strip_width: 96,
            height: 8,
            stride_bytes: 12,
            char_x: [0; 96],
            char_w: [4; 96],
            bitmap: vec![0; 8 * 12],
        };
        let mut audio_status = AudioStatus::default();
        sync_audio_status(&audio, &mut audio_status);

        let mut app = App::new();
        app.add_plugins((
            CoreGamePlugin {
                initial_screen: AppScreen::Playing,
            },
            AudioSyncPlugin,
            BrowserPlugin,
            SetupPlugin,
            PuzzlePlugin,
            RenderTrackingPlugin,
        ));
        app.insert_resource(GameConfig {
            game_dir: PathBuf::new(),
            prefs,
            session_use_ega: asset.uses_ega(),
        })
        .insert_resource(StaticAssets {
            font,
            tbx,
            tbx_ega,
            title_images: Vec::new(),
            credit_images: Vec::new(),
        })
        .insert_resource(BrowserState {
            selected_puzzle: puzzle_num,
            preview_asset: asset.clone(),
            selected_grid_dim: grid_dim,
            curved_edges,
            status: None,
            hover: None,
            pressed: None,
        })
        .insert_resource(SetupState {
            menu: SetupMenu::new(prefs),
        })
        .insert_resource(audio_status)
        .insert_non_send(audio);
        app.finish();
        app.cleanup();

        let mut asset_slot = Some(asset);
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut active_puzzle: ResMut<ActivePuzzle>,
                      piece_q: Query<Entity, With<PieceIdentity>>,
                      hotspot_q: Query<Entity, With<SceneHotspot>>,
                      anim_q: Query<Entity, With<AnimationState>>| {
                    let asset = asset_slot.take().unwrap();
                    spawn_puzzle_session(
                        &mut commands,
                        &mut active_puzzle,
                        &piece_q,
                        &hotspot_q,
                        &anim_q,
                        puzzle_num,
                        asset,
                        grid_dim,
                        curved_edges,
                        grid_lines,
                        timer_enabled,
                        prefs.text_msg_on != 0,
                    );
                },
            )
            .unwrap();

        let mut h = Self { app };
        // Run one 0-dt tick to initialize RenderVersions for the freshly spawned session.
        h.step(AppInput::default(), 0.0);
        h
    }

    fn new_no_audio(
        puzzle_num: usize,
        asset: PuzzleAsset,
        grid_dim: usize,
        curved_edges: bool,
        grid_lines: bool,
        timer_enabled: bool,
    ) -> Self {
        let audio = AudioEngine::new(Path::new(""), false, false);
        Self::new(
            puzzle_num,
            asset,
            grid_dim,
            curved_edges,
            grid_lines,
            timer_enabled,
            audio,
        )
    }

    fn world(&self) -> &World {
        self.app.world()
    }

    fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    fn audio(&self) -> &AudioEngine {
        self.app.world().non_send::<AudioEngine>()
    }

    fn audio_mut(&mut self) -> &mut AudioEngine {
        self.app.world_mut().non_send_mut::<AudioEngine>().into_inner()
    }

    fn sync_audio_status(&mut self) {
        let mut status = AudioStatus::default();
        sync_audio_status(self.audio(), &mut status);
        *self.world_mut().resource_mut::<AudioStatus>() = status;
    }

    fn step(&mut self, mut input: AppInput, dt: f32) {
        input.dt = dt;
        step_app(&mut self.app, &input);
    }

    fn screen(&self) -> AppScreen {
        *self.world().resource::<State<AppScreen>>().get()
    }

    fn meta(&self) -> &PuzzleMeta {
        self.world().resource::<ActivePuzzle>().0.as_ref().unwrap()
    }

    fn meta_mut(&mut self) -> &mut PuzzleMeta {
        self.world_mut()
            .resource_mut::<ActivePuzzle>()
            .into_inner()
            .0
            .as_mut()
            .unwrap()
    }

    fn render_versions(&self) -> RenderVersions {
        *self.world().resource::<RenderVersions>()
    }

    fn handle_pointer_press(&mut self, x: i32, y: i32) {
        self.step(
            AppInput {
                pointer_x: x,
                pointer_y: y,
                left_pressed: true,
                ..Default::default()
            },
            0.0,
        );
    }

    fn handle_pointer_release(&mut self, x: i32, y: i32) {
        self.step(
            AppInput {
                pointer_x: x,
                pointer_y: y,
                left_released: true,
                ..Default::default()
            },
            0.0,
        );
    }

    fn handle_click(&mut self, x: i32, y: i32) {
        self.handle_pointer_press(x, y);
        self.handle_pointer_release(x, y);
    }

    fn handle_right_click(&mut self, x: i32, y: i32) {
        self.step(
            AppInput {
                pointer_x: x,
                pointer_y: y,
                right_pressed: true,
                ..Default::default()
            },
            0.0,
        );
    }

    fn handle_keyboard_activate(&mut self) -> AppScreen {
        self.step(
            AppInput {
                enter: true,
                ..Default::default()
            },
            0.0,
        );
        self.screen()
    }

    fn move_keyboard_pointer(&mut self, dx: f32, dy: f32, dt: f32) {
        move_keyboard_pointer_meta(self.meta_mut(), dx, dy, dt);
    }

    fn update(&mut self, dt: f32) {
        self.step(AppInput::default(), dt);
    }

    fn scroll_tray(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let step_dir = if delta > 0 { -1 } else { 1 };
        let steps = (delta as i64).abs().min(64);
        for _ in 0..steps {
            self.step(
                AppInput {
                    wheel_delta: step_dir,
                    ..Default::default()
                },
                0.0,
            );
        }
    }

    fn toggle_toolbox(&mut self) {
        self.step(
            AppInput {
                space: true,
                ..Default::default()
            },
            0.0,
        );
    }

    fn toggle_grid_lines(&mut self) {
        self.step(
            AppInput {
                key_g: true,
                ..Default::default()
            },
            0.0,
        );
    }

    fn trigger_hint(&mut self) {
        self.step(
            AppInput {
                f1: true,
                ..Default::default()
            },
            0.0,
        );
    }

    fn cancel_pointer_drag(&mut self) {
        cancel_pointer_drag(self.world_mut());
    }

    fn held_piece(&mut self) -> Option<usize> {
        self.world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(self.world())
            .find_map(|(id, loc)| match *loc {
                PieceLocation::Held { is_anchor: true, .. } => Some(id.index),
                _ => None,
            })
    }

    fn held_group(&mut self) -> Vec<usize> {
        let mut items: Vec<(usize, usize)> = self
            .world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(self.world())
            .filter_map(|(id, loc)| match *loc {
                PieceLocation::Held { group_order, .. } => Some((group_order, id.index)),
                _ => None,
            })
            .collect();
        items.sort_by_key(|&(go, idx)| (go, idx));
        items.into_iter().map(|(_, idx)| idx).collect()
    }

    fn tray_order(&mut self) -> Vec<usize> {
        let mut items: Vec<(usize, usize)> = self
            .world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(self.world())
            .filter_map(|(id, loc)| match *loc {
                PieceLocation::Tray { order } => Some((order, id.index)),
                _ => None,
            })
            .collect();
        items.sort_by_key(|&(ord, idx)| (ord, idx));
        items.into_iter().map(|(_, idx)| idx).collect()
    }

    fn is_placed(&mut self, idx: usize) -> bool {
        self.world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(self.world())
            .any(|(id, loc)| id.index == idx && loc.is_placed())
    }

    fn placed_count(&mut self) -> usize {
        self.world_mut()
            .query::<&PieceLocation>()
            .iter(self.world())
            .filter(|loc| loc.is_placed())
            .count()
    }

    fn piece_count(&mut self) -> usize {
        self.world_mut()
            .query::<&PieceIdentity>()
            .iter(self.world())
            .count()
    }

    fn piece_geom(&mut self, idx: usize) -> PieceGeometry {
        self.world_mut()
            .query::<(&PieceIdentity, &PieceGeometry)>()
            .iter(self.world())
            .find_map(|(id, g)| (id.index == idx).then_some(*g))
            .unwrap()
    }

    fn piece_sprite(&mut self, idx: usize) -> Sprite8061 {
        self.world_mut()
            .query::<(&PieceIdentity, &PieceSprite)>()
            .iter(self.world())
            .find_map(|(id, s)| (id.index == idx).then(|| s.sprite.clone()))
            .unwrap()
    }

    fn piece_identity(&mut self, idx: usize) -> PieceIdentity {
        self.world_mut()
            .query::<&PieceIdentity>()
            .iter(self.world())
            .find_map(|id| (id.index == idx).then_some(*id))
            .unwrap()
    }

    fn board_cell(&mut self, r: usize, c: usize) -> Option<usize> {
        self.world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(self.world())
            .find_map(|(id, loc)| {
                (*loc == PieceLocation::Board { row: r, col: c }).then_some(id.index)
            })
    }

    fn active_anim(&mut self) -> Option<AnimationState> {
        self.world_mut()
            .query::<&AnimationState>()
            .iter(self.world())
            .next()
            .cloned()
    }

    fn set_active_anim(&mut self, anim: Option<AnimationState>) {
        let existing: Vec<Entity> = self.world_mut()
            .query_filtered::<Entity, With<AnimationState>>()
            .iter(self.world())
            .collect();
        for e in existing {
            self.world_mut().entity_mut(e).despawn();
        }
        if let Some(a) = anim {
            self.world_mut().spawn(a);
        }
    }

    fn hint_piece(&mut self) -> Option<(usize, f32)> {
        let w = self.world_mut();
        w.query::<(&PieceIdentity, &HintHighlight)>()
            .iter(w)
            .next()
            .map(|(id, h)| (id.index, h.timer.remaining_secs()))
    }

    fn set_piece_placed(&mut self, idx: usize, placed: bool) {
        let mut target = None;
        let mut max_order = None::<usize>;
        for (e, id, loc) in self
            .world_mut()
            .query::<(Entity, &PieceIdentity, &PieceLocation)>()
            .iter(self.world())
        {
            if id.index == idx {
                target = Some(e);
            }
            if let PieceLocation::Tray { order } = *loc {
                max_order = Some(max_order.map_or(order, |m| m.max(order)));
            }
        }
        let e = target.unwrap();
        let ident = self.piece_identity(idx);
        let mut loc = self.world_mut().get_mut::<PieceLocation>(e).unwrap();
        if placed {
            *loc = PieceLocation::Board {
                row: ident.home_row,
                col: ident.home_col,
            };
        } else {
            *loc = PieceLocation::Tray {
                order: max_order.map_or(0, |m| m + 1),
            };
        }
        self.step(AppInput::default(), 0.0);
    }

    fn set_all_placed(&mut self) {
        let items: Vec<(Entity, usize, usize)> = self
            .world_mut()
            .query::<(Entity, &PieceIdentity)>()
            .iter(self.world())
            .map(|(e, id)| (e, id.home_row, id.home_col))
            .collect();
        for (e, r, c) in items {
            *self.world_mut().get_mut::<PieceLocation>(e).unwrap() =
                PieceLocation::Board { row: r, col: c };
        }
        self.step(AppInput::default(), 0.0);
    }

    fn truncate_tray(&mut self, keep_len: usize) {
        let tray = self.tray_order();
        let to_place: Vec<usize> = tray.into_iter().skip(keep_len).collect();
        for idx in to_place {
            self.set_piece_placed(idx, true);
        }
    }

    fn start_sequence(&mut self, playback: SequencePlayback) {
        self.sync_audio_status();
        self.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut active_puzzle: ResMut<ActivePuzzle>,
                      audio_status: Res<AudioStatus>,
                      anim_q: Query<Entity, With<AnimationState>>,
                      mut audio_out: MessageWriter<AudioCommand>| {
                    let Some(meta) = &mut active_puzzle.0 else {
                        return;
                    };
                    for e in anim_q.iter() {
                        commands.entity(e).despawn();
                    }
                    let mut cmds = Vec::new();
                    if let Some(anim) =
                        build_animation_state(meta, playback, &audio_status, &mut cmds)
                    {
                        commands.spawn(anim);
                    }
                    flush_audio_commands(cmds, &mut audio_out);
                },
            )
            .unwrap();
        self.step(AppInput::default(), 0.0);
    }

    fn trigger_frame_side_effects(&mut self, frame_idx: usize) {
        self.sync_audio_status();
        self.world_mut()
            .run_system_once(
                move |mut active_puzzle: ResMut<ActivePuzzle>,
                      audio_status: Res<AudioStatus>,
                      mut anim_q: Query<&mut AnimationState>,
                      mut audio_out: MessageWriter<AudioCommand>| {
                    let Some(meta) = &mut active_puzzle.0 else {
                        return;
                    };
                    if let Some(mut anim) = anim_q.iter_mut().next() {
                        let mut cmds = Vec::new();
                        apply_frame_side_effects(
                            &mut meta.asset,
                            &mut meta.voice_rng,
                            meta.text_enabled,
                            &audio_status,
                            &mut anim,
                            frame_idx,
                            &mut cmds,
                        );
                        flush_audio_commands(cmds, &mut audio_out);
                    }
                },
            )
            .unwrap();
    }

    fn on_puzzle_completed(&mut self) {
        self.sync_audio_status();
        self.world_mut()
            .run_system_once(
                |mut commands: Commands,
                 mut active_puzzle: ResMut<ActivePuzzle>,
                 audio_status: Res<AudioStatus>,
                 anim_q: Query<Entity, With<AnimationState>>,
                 hint_q: Query<Entity, With<HintHighlight>>,
                 mut audio_out: MessageWriter<AudioCommand>| {
                    let Some(meta) = &mut active_puzzle.0 else {
                        return;
                    };
                    let existing_anims: Vec<Entity> = anim_q.iter().collect();
                    let existing_hints: Vec<Entity> = hint_q.iter().collect();
                    complete_puzzle(
                        meta,
                        &audio_status,
                        &mut commands,
                        &existing_anims,
                        &existing_hints,
                        &mut audio_out,
                    );
                },
            )
            .unwrap();
        self.step(AppInput::default(), 0.0);
    }

    fn replace_scene_asset(&mut self, asset: PuzzleAsset) {
        let mut slot = Some(asset);
        self.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut active_puzzle: ResMut<ActivePuzzle>,
                      mut piece_q: Query<(Entity, &PieceIdentity, &mut PieceGeometry, &mut PieceSprite)>,
                      hotspot_q: Query<Entity, With<SceneHotspot>>,
                      anim_q: Query<Entity, With<AnimationState>>| {
                    let Some(meta) = &mut active_puzzle.0 else {
                        return;
                    };
                    let asset = slot.take().unwrap();
                    replace_scene_asset_ecs(
                        &mut commands,
                        meta,
                        asset,
                        &mut piece_q,
                        &hotspot_q,
                        &anim_q,
                    );
                },
            )
            .unwrap();
        self.step(AppInput::default(), 0.0);
    }

    fn solved_sequence_at(&mut self, sx: i32, sy: i32) -> SequencePlayback {
        let hotspots: Vec<SceneHotspot> = self
            .world_mut()
            .query::<&SceneHotspot>()
            .iter(self.world())
            .cloned()
            .collect();
        let meta = self.meta();
        solved_sequence_at(&meta.asset, hotspots.iter(), sx, sy)
    }

    fn render(
        &mut self,
        renderer: &mut PuzzleRenderer,
        fb: &mut Framebuffer,
        tbx: &ToolboxAsset,
        font: &FontAsset,
        mx: i32,
        my: i32,
    ) {
        renderer.render(self.world_mut(), fb, tbx, font, mx, my);
    }

    fn present_key(
        &mut self,
        picture_guide: bool,
        mx: i32,
        my: i32,
    ) -> PuzzlePresentKey {
        puzzle_present_key(self.world_mut(), mx, my, picture_guide).unwrap()
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn tray_bottom_padding_selects_the_last_visible_piece() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for dim in 1..=8 {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let audio = AudioEngine::new(dir, false, false);
        let mut session = EcsPuzzleHarness::new(1, asset, dim, true, false, false, audio);
        let slots = session.meta().visible_tray_slots();
        let tray_len = session.tray_order().len();
        for scroll in [0, 1, tray_len - slots] {
            for y in [184, 185] {
                session.meta_mut().tray_scroll = scroll;
                let original = session.tray_order();
                let last_visible = *original[scroll..scroll + slots].last().unwrap();
                session.handle_pointer_press(20, y);
                session.handle_pointer_release(20, y);
                assert_eq!(
                    session.held_piece(),
                    Some(last_visible),
                    "dim={dim} scroll={scroll} y={y}"
                );
                session.handle_pointer_press(20, y);
                session.handle_pointer_release(20, y);
                assert!(session.held_piece().is_none());
                assert_eq!(session.tray_order(), original);
                session.meta_mut().keyboard_pointer = Some((20.0, y as f32));
                assert_ne!(session.handle_keyboard_activate(), AppScreen::Browser);
                assert_eq!(session.held_piece(), Some(last_visible));
                assert_ne!(session.handle_keyboard_activate(), AppScreen::Browser);
                assert!(session.held_piece().is_none());
                assert_eq!(session.tray_order(), original);
            }
        }
        session.meta_mut().tray_scroll = 0;
        let original = session.tray_order();
        session.handle_pointer_press(20, 186);
        session.handle_pointer_release(20, 186);
        assert!(session.held_piece().is_none());
        assert_eq!(session.meta().tray_scroll, 1);
        assert_eq!(session.tray_order(), original);
        session.meta_mut().tray_scroll = 0;
        session.truncate_tray(slots - 1);
        let original = session.tray_order();
        session.handle_pointer_press(20, 185);
        session.handle_pointer_release(20, 185);
        assert!(session.held_piece().is_none());
        assert_eq!(session.tray_order(), original);
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn projector_dissolve_starts_from_the_presented_picture() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        let asset = PuzzleAsset::load(dir, 5, ega).unwrap();
        let toolbox =
            ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega())).unwrap();
        let audio = AudioEngine::new(dir, false, false);
        let mut session = EcsPuzzleHarness::new(5, asset, 2, false, false, false, audio);
        session.meta_mut().solved = true;
        let playback = session
            .meta()
            .asset
            .hotspots
            .iter()
            .find(|h| h.playback.seq_idx == 8)
            .unwrap()
            .playback;
        let mut expected = Framebuffer::new();
        render_scene_view(
            &mut expected,
            &session.meta().asset,
            session.meta().asset.sequences[1].start_frame,
        );
        let mut renderer = PuzzleRenderer::new();
        session.start_sequence(playback);
        let mut actual = Framebuffer::new();
        session.render(&mut renderer, &mut actual, &toolbox, &font, 0, 0);
        for y in 2..198 {
            for x in 25..295 {
                assert_eq!(
                    actual.get_pixel(x, y),
                    expected.get_pixel(x, y),
                    "ega={ega} {x},{y}"
                );
            }
        }
        let anim = session.active_anim().unwrap();
        assert_eq!(anim.transition_mode, 8);
        let frame = anim.cur_frame;
        let frame_delay = anim.frame_delay_secs;
        let duration = frame_delay.min(4.0 * DOS_TICK_SECS);
        let mut next = Framebuffer::new();
        render_scene_view(&mut next, &session.meta().asset, frame);
        session.update(duration / 2.0);
        session.render(&mut renderer, &mut actual, &toolbox, &font, 0, 0);
        let mut revealed = vec![false; 64000];
        for &pixel in crate::graphics::dissolve_order(ega).iter().take(32000) {
            revealed[pixel as usize] = true;
        }
        let (mut old_pixels, mut new_pixels) = (0, 0);
        for y in 2..198 {
            for x in 25..295 {
                let pixel = (y * 320 + x - SCENE_OFFSET_X) as usize;
                let before = expected.get_pixel(x, y);
                let after = next.get_pixel(x, y);
                if before != after {
                    if revealed[pixel] {
                        new_pixels += 1
                    } else {
                        old_pixels += 1
                    }
                }
                let color = if revealed[pixel] { after } else { before };
                assert_eq!(actual.get_pixel(x, y), color, "ega={ega} midpoint {x},{y}");
            }
        }
        assert!(old_pixels > 0 && new_pixels > 0);
        session.update(duration / 2.0);
        session.render(&mut renderer, &mut actual, &toolbox, &font, 0, 0);
        for y in 2..198 {
            for x in 25..295 {
                assert_eq!(actual.get_pixel(x, y), next.get_pixel(x, y));
            }
        }
        session.update(frame_delay - duration);
        let anim = session.active_anim().unwrap();
        assert_eq!(anim.previous_frame, frame);
        assert_eq!(anim.cur_frame, frame + 1);
        assert_eq!(anim.transition_mode, 8);
        let delay = anim.frame_delay_secs;
        session.update(delay * 0.75);
        session.render(&mut renderer, &mut actual, &toolbox, &font, 0, 0);
        render_scene_view(&mut next, &session.meta().asset, frame + 1);
        for y in 2..198 {
            for x in 25..295 {
                assert_eq!(
                    actual.get_pixel(x, y),
                    next.get_pixel(x, y),
                    "ega={ega} final frame {x},{y}"
                );
            }
        }
        session.handle_click(100, 100);
        assert!(session.active_anim().is_none());
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn robot_hotspot_repeats_its_native_sequence() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 2, false).unwrap();
    let audio = AudioEngine::new(dir, false, false);
    let mut session = EcsPuzzleHarness::new(2, asset, 2, false, false, false, audio);
    let playback = session
        .meta()
        .asset
        .hotspots
        .iter()
        .find(|h| h.playback.seq_idx == 4)
        .unwrap()
        .playback;
    session.start_sequence(playback);
    let start = session.active_anim().unwrap().cur_frame;
    let length = session.active_anim().unwrap().end_frame - start;
    for _ in 0..length {
        let delay = session.active_anim().unwrap().frame_delay_secs;
        session.update(delay);
    }
    assert_eq!(
        session.active_anim().map(|a| a.cur_frame),
        Some(start)
    );
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn sequence_repetitions_and_delays_match_original_machine_code() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    for row in include_str!("../validation/sequence-playback.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        let repetitions: u16 = fields[0].parse().unwrap();
        let ticks: u16 = fields[1].parse().unwrap();
        let explicit: u16 = fields[2].parse().unwrap();
        let frames: Vec<usize> = fields[3].split(',').map(|v| v.parse().unwrap()).collect();
        let delays: Vec<u16> = fields[4].split(',').map(|v| v.parse().unwrap()).collect();
        let mut asset = PuzzleAsset::load(dir, 1, false).unwrap();
        asset.records = vec![SceneRecord::SequenceHeader {
            name: "FILM".into(),
            default_playback: Some(SequencePlayback {
                seq_idx: 2,
                repetitions,
                frame_delay_ticks: ticks,
            }),
            hotspots: Vec::new(),
        }];
        for frame in 1..=3 {
            asset.records.push(SceneRecord::Frame {
                bg_frame_idx: 0,
                bg_color: 0,
                commands: if frame == 2 && explicit != 0 {
                    vec![FrameCommand::Delay {
                        raw: explicit << 6,
                        ticks: explicit,
                    }]
                } else {
                    Vec::new()
                },
            });
        }
        asset.sequences[1].header_idx = 0;
        asset.sequences[2].start_frame = 1;
        asset.sequences[2].end_frame = 4;
        let audio = AudioEngine::new(dir, false, false);
        let mut session = EcsPuzzleHarness::new(1, asset, 2, false, false, false, audio);
        session.on_puzzle_completed();
        assert_eq!(frames.len(), delays.len());
        for (frame, delay) in frames.into_iter().zip(delays) {
            let anim = session.active_anim().unwrap();
            assert_eq!(anim.cur_frame, frame, "{row}");
            assert_eq!(
                anim.frame_delay_secs,
                f32::from(delay) * DOS_TICK_SECS,
                "{row}"
            );
            session.update(anim.frame_delay_secs);
        }
        assert!(session.active_anim().is_none(), "{row}");
        checked += 1;
    }
    assert_eq!(checked, 48);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn original_preset_pieces_can_be_placed_in_both_video_modes() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for ega in [false, true] {
        for num in 1..=15 {
            let asset = PuzzleAsset::load(dir, num, ega).unwrap();
            let audio = AudioEngine::new(dir, false, false);
            let mut session = EcsPuzzleHarness::new(num, asset, 1, true, true, false, audio);
            assert_eq!(session.meta().grid_dim, 1);
            let count = session.piece_count();
            assert!((11..=16).contains(&count));
            for ordinal in 0..count {
                session.handle_pointer_press(20, 40);
                let held = session.held_piece().unwrap();
                let piece = session.piece_geom(held);
                let sprite = session.piece_sprite(held);
                assert!(sprite.opacity.as_ref().unwrap().iter().any(|&v| v));
                let (x, y) = (
                    session.meta().playfield_x() + piece.base_x + piece.base_w / 2,
                    session.meta().playfield_y() + piece.base_y + piece.base_h / 2,
                );
                if ordinal % 2 == 0 {
                    session.handle_pointer_release(x, y);
                } else {
                    session.handle_pointer_release(20, 40);
                    session.handle_pointer_press(x, y);
                    session.handle_pointer_release(x, y);
                }
                assert!(
                    session.is_placed(held),
                    "P{num:02} ega={ega} piece={held}"
                );
            }
            assert!(session.meta().solved);
        }
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn preset_board_keeps_native_layers_and_viewport_clipping() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        for num in 1..=15 {
            let asset = PuzzleAsset::load(dir, num, ega).unwrap();
            let toolbox =
                ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega())).unwrap();
            let mut session = EcsPuzzleHarness::new_no_audio(num, asset, 1, true, true, false);
            let mut renderer = PuzzleRenderer::new();
            let count = session.piece_count();
            for phase in 0..8 {
                session.meta_mut().toolbox_open = phase < 4;
                let phase = phase % 4;
                for idx in 0..count {
                    let placed = phase == 3 || (phase != 0 && idx % 2 == phase - 1);
                    session.set_piece_placed(idx, placed);
                    let piece = session.piece_geom(idx);
                    assert!(piece.base_w > 0 && piece.base_h > 0);
                    assert!(piece.base_x >= 0 && piece.base_y >= 0);
                    assert!(piece.base_x + piece.base_w <= PLAYFIELD_W);
                    assert!(piece.base_y + piece.base_h <= PLAYFIELD_H);
                }
                let mut expected_asset = session.meta().asset.clone();
                let frame_idx = session.meta().asset.preset_frame_idx();
                if let SceneRecord::Frame { commands, .. } = &mut expected_asset.records[frame_idx]
                {
                    for (idx, command) in commands.iter_mut().enumerate() {
                        if let FrameCommand::SubFrame { flags, .. } = command {
                            *flags = if session.is_placed(idx) {
                                *flags & 3
                            } else {
                                (*flags & 0xff0f) | 0x20
                            };
                        }
                    }
                }
                let mut expected = Framebuffer::new();
                render_scene_frame(
                    &mut expected,
                    &expected_asset,
                    frame_idx,
                    0,
                    0,
                    0,
                    true,
                    0,
                    0,
                );
                let mut actual = Framebuffer::new();
                session.render(&mut renderer, &mut actual, &toolbox, &font, 0, 0);
                for y in 2..198 {
                    for x in 2..272 {
                        assert_eq!(
                            actual.get_pixel(session.meta().playfield_x() + x - 2, y),
                            expected.get_pixel(x, y),
                            "P{num:02} ega={ega} phase={phase} {x},{y}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn completed_picture_matches_the_assembled_board_pixels() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        for id in 1..=15 {
            let asset = PuzzleAsset::load(dir, id, ega).unwrap();
            let toolbox =
                ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega())).unwrap();
            let mut session = EcsPuzzleHarness::new_no_audio(id, asset, 3, true, false, false);
            session.meta_mut().toolbox_open = false;
            session.set_all_placed();
            let mut renderer = PuzzleRenderer::new();
            let mut board = Framebuffer::new();
            session.render(&mut renderer, &mut board, &toolbox, &font, 0, 0);
            session.meta_mut().solved = true;
            session.step(AppInput::default(), 0.0);
            let mut picture = Framebuffer::new();
            session.render(&mut renderer, &mut picture, &toolbox, &font, 0, 0);
            for y in 2..198 {
                for x in 25..295 {
                    assert_eq!(
                        board.get_pixel(x, y),
                        picture.get_pixel(x, y),
                        "P{id:02} ega={ega} {x},{y}"
                    );
                }
            }
        }
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn grid_pieces_match_original_capture_rectangles_and_source_pixels() {
    let asset = PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 1, false).unwrap();
    let mut sessions: Vec<_> = (2..=8)
        .map(|dim| EcsPuzzleHarness::new_no_audio(1, asset.clone(), dim, false, false, false))
        .collect();
    let mut checked = 0;
    for row in include_str!("../validation/piece-capture.tsv")
        .lines()
        .skip(1)
    {
        let v: Vec<i32> = row.split('\t').map(|word| word.parse().unwrap()).collect();
        let session = &mut sessions[v[0] as usize - 2];
        let piece = session.piece_geom(v[1] as usize);
        let sprite = session.piece_sprite(v[1] as usize);
        assert_eq!(
            (
                2 + piece.base_x,
                2 + piece.base_y,
                piece.base_w,
                piece.base_h
            ),
            (v[2], v[3], v[4], v[5]),
            "{row}"
        );
        for y in 0..sprite.sub_h as i32 {
            for x in 0..sprite.width as i32 {
                let index = (y * sprite.stride as i32 + x) as usize;
                if sprite.is_opaque(index) {
                    let source_x = v[2] + piece.target_x - piece.base_x + x;
                    let source_y = v[3] + piece.target_y - piece.base_y + y;
                    assert_eq!(
                        sprite.pixels[index],
                        session.meta().full_picture_fb.get_pixel(source_x, source_y),
                        "{row}: {x},{y}"
                    );
                }
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 203);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn keyboard_pointer_can_complete_every_grid_and_yield_to_mouse() {
    fn move_to(session: &mut EcsPuzzleHarness, x: i32, y: i32) {
        let (old_x, old_y) = session.meta().keyboard_pointer.unwrap_or((20.0, 40.0));
        let dx = x as f32 - old_x;
        let dy = y as f32 - old_y;
        session.move_keyboard_pointer(dx.signum(), 0.0, dx.abs() / 120.0);
        session.move_keyboard_pointer(0.0, dy.signum(), dy.abs() / 120.0);
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for dim in 1..=8 {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let audio = AudioEngine::new(dir, false, false);
        let mut session = EcsPuzzleHarness::new(1, asset, dim, true, false, false, audio);
        let count = session.piece_count();
        for _ in 0..count {
            move_to(&mut session, 20, 40);
            assert_ne!(session.handle_keyboard_activate(), AppScreen::Browser);
            let held = session.held_piece().unwrap();
            let piece = session.piece_geom(held);
            let (x, y) = (
                session.meta().playfield_x() + piece.base_x + piece.base_w / 2,
                session.meta().playfield_y() + piece.base_y + piece.base_h / 2,
            );
            move_to(&mut session, x, y);
            session.handle_keyboard_activate();
            assert!(session.is_placed(held));
        }
        assert!(session.meta().solved);
        session.handle_keyboard_activate();
        assert!(session.active_anim().is_none());
        move_to(&mut session, 2, 2);
        assert_ne!(session.handle_keyboard_activate(), AppScreen::Browser);
        assert!(session.world().resource::<InGameMenuState>().open);
        session.world_mut().resource_mut::<InGameMenuState>().open = false;
        session.move_keyboard_pointer(-1.0, -1.0, 100.0);
        assert_eq!(session.meta().keyboard_pointer, Some((0.0, 0.0)));
        session.move_keyboard_pointer(1.0, 1.0, 100.0);
        assert_eq!(session.meta().keyboard_pointer, Some((319.0, 199.0)));
        session.handle_pointer_press(2, 2);
        assert!(session.meta().keyboard_pointer.is_none());
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn speech_captions_survive_muting_both_audio_options() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let audio = AudioEngine::new(dir, false, false);
    let mut checked = 0;
    for ega in [false, true] {
        for id in 1..=15 {
            let asset = PuzzleAsset::load(dir, id, ega).unwrap();
            let candidates: Vec<_> = asset
                .sounds
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    let music = entry.sounds.first()?;
                    (audio
                        .iss_songs
                        .contains_key(&(music.stem.clone(), music.phrase_idx as usize))
                        && !entry.bubbles.is_empty()
                        && entry.bubbles.iter().all(Option::is_some))
                    .then_some(index)
                })
                .collect();
            let audio_local = AudioEngine::new(dir, false, false);
            let mut session = EcsPuzzleHarness::new(id, asset, 2, false, false, false, audio_local);
            session.meta_mut().text_enabled = true;
            for sound in candidates {
                let frame = session.meta().asset.records.iter().position(|record| matches!(record,
                    SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                        matches!(command, FrameCommand::Sound { snd_idx: Some(index) } if *index == sound)))).unwrap();
                session.set_active_anim(Some(AnimationState {
                    seq_idx: 0,
                    start_frame: frame,
                    previous_frame: frame,
                    cur_frame: frame,
                    end_frame: frame + 1,
                    tick_accum: 0.0,
                    frame_delay_secs: 0.0,
                    repetitions_left: 1,
                    default_delay_secs: 2.0 * DOS_TICK_SECS,
                    transition_mode: 0,
                    bubble: None,
                    wait_for_audio: false,
                    looping_sound: None,
                }));
                session.trigger_frame_side_effects(frame);
                assert!(
                    session.active_anim().unwrap().bubble.is_some(),
                    "P{id:02} ega={ega} sound={}",
                    session.meta().asset.sounds[sound].tag
                );
                session.meta_mut().text_enabled = false;
                session.trigger_frame_side_effects(frame);
                assert!(session.active_anim().unwrap().bubble.is_none());
                session.meta_mut().text_enabled = true;
                session.audio_mut().music_enabled = true;
                session.trigger_frame_side_effects(frame);
                assert!(session.active_anim().unwrap().bubble.is_none());
                session.audio_mut().music_enabled = false;
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 2);
    println!("{checked} music/speech entries retain captions with both channels muted");
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn tray_toggle_cancels_drag_without_losing_held_piece() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for dim in 2..=8 {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let audio = AudioEngine::new(dir, false, false);
        let mut session = EcsPuzzleHarness::new(1, asset, dim, true, true, true, audio);
        session.handle_pointer_press(20, 40);
        let held = session.held_piece().unwrap();
        session.toggle_toolbox();
        let piece = session.piece_geom(held);
        let target = (
            session.meta().playfield_x() + piece.base_x + piece.base_w / 2,
            session.meta().playfield_y() + piece.base_y + piece.base_h / 2,
        );
        session.handle_pointer_release(target.0, target.1);
        assert_eq!(session.held_piece(), Some(held));
        assert!(!session.is_placed(held));
        assert!(!session.tray_order().contains(&held));
        session.handle_pointer_press(target.0, target.1);
        assert!(session.is_placed(held));
        session.handle_pointer_release(target.0, target.1);
        session.toggle_toolbox();
        assert!(session.meta().toolbox_open);
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn drag_release_places_pieces_without_breaking_click_to_carry() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for dim in 2..=8 {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let audio = AudioEngine::new(dir, false, false);
        let mut session = EcsPuzzleHarness::new(1, asset, dim, true, true, true, audio);
        session.handle_pointer_press(20, 40);
        let held = session.held_piece().unwrap();
        session.handle_pointer_release(21, 41);
        assert_eq!(session.held_piece(), Some(held));
        let piece = session.piece_geom(held);
        let target = (
            session.meta().playfield_x() + piece.base_x + piece.base_w / 2,
            session.meta().playfield_y() + piece.base_y + piece.base_h / 2,
        );
        session.handle_pointer_press(target.0, target.1);
        session.handle_pointer_release(target.0, target.1);
        assert!(session.is_placed(held));
        for _ in 1..dim * dim {
            session.handle_pointer_press(20, 40);
            let held = session.held_piece().unwrap();
            let piece = session.piece_geom(held);
            let target = (
                session.meta().playfield_x() + piece.base_x + piece.base_w / 2,
                session.meta().playfield_y() + piece.base_y + piece.base_h / 2,
            );
            session.handle_pointer_release(target.0, target.1);
            assert!(session.is_placed(held));
        }
        assert!(session.meta().solved);
        assert!(session.tray_order().is_empty());
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn drag_release_outside_or_over_controls_keeps_the_piece() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for release in [
        (-1, 40),
        (320, 40),
        (20, -1),
        (20, 200),
        (10, 10),
        (10, 20),
        (10, 190),
    ] {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let audio = AudioEngine::new(dir, false, false);
        let mut session = EcsPuzzleHarness::new(1, asset, 3, true, true, true, audio);
        session.handle_pointer_press(20, 40);
        let held = session.held_piece().unwrap();
        session.handle_pointer_release(release.0, release.1);
        assert_eq!(session.held_piece(), Some(held), "{release:?}");
        assert!(!session.is_placed(held));
        assert!(!session.tray_order().contains(&held));
        assert_eq!(session.meta().tray_scroll, 0);
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn cancelled_drag_keeps_piece_and_tray_drop_returns_it_once() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let audio = AudioEngine::new(dir, false, false);
    let mut session = EcsPuzzleHarness::new(1, asset, 3, true, true, true, audio);
    session.handle_pointer_press(20, 40);
    let held = session.held_piece().unwrap();
    let piece = session.piece_geom(held);
    let target = (
        session.meta().playfield_x() + piece.base_x + piece.base_w / 2,
        session.meta().playfield_y() + piece.base_y + piece.base_h / 2,
    );
    session.cancel_pointer_drag();
    session.handle_pointer_release(target.0, target.1);
    assert_eq!(session.held_piece(), Some(held));
    assert!(!session.is_placed(held));
    session.handle_pointer_press(20, 40);
    assert!(session.held_piece().is_none());
    session.handle_pointer_release(20, 40);
    session.handle_pointer_press(20, 40);
    let held = session.held_piece().unwrap();
    session.handle_pointer_release(20, 130);
    assert!(session.held_piece().is_none());
    assert_eq!(
        session
            .tray_order()
            .iter()
            .filter(|&&index| index == held)
            .count(),
        1
    );
    assert_eq!(session.tray_order().len(), 9);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn supplied_repeated_effects_survive_frames_and_cancel() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (id, ega, tag) in [
        (2, false, "ROBOT"),
        (2, true, "ROBOT"),
        (6, false, "SKATE"),
        (13, false, "HORSWALK"),
        (13, true, "HORSWALK"),
    ] {
        let mut asset = PuzzleAsset::load(dir, id, ega).unwrap();
        let sound = asset
            .sounds
            .iter()
            .position(|entry| entry.tag == tag)
            .unwrap();
        let frame = asset.records.iter().position(|record| matches!(record,
            SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                matches!(command, FrameCommand::Sound { snd_idx: Some(index) } if *index == sound))
        )).unwrap();
        asset.sequences[2].start_frame = frame;
        asset.sequences[2].end_frame = frame + 2;
        let mut audio = AudioEngine::new(dir, false, false);
        audio.sfx_enabled = true;
        let mut session = EcsPuzzleHarness::new(id, asset, 2, false, false, false, audio);
        session.on_puzzle_completed();
        assert_eq!(
            session.active_anim().unwrap().looping_sound,
            Some((sound, 1)),
            "{id} {ega}"
        );
        let delay = session.active_anim().unwrap().frame_delay_secs;
        session.update(delay);
        assert_eq!(
            session.active_anim().unwrap().looping_sound,
            Some((sound, 1)),
            "{id} {ega}"
        );
        let replacement = session
            .meta()
            .asset
            .records
            .iter()
            .position(|record| {
                matches!(record,
                    SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                        matches!(command, FrameCommand::Sound { snd_idx: Some(index) }
                            if session.meta().asset.sounds[*index].selection_mode & 0x80 == 0))
                )
            })
            .unwrap();
        session.trigger_frame_side_effects(replacement);
        assert!(
            session
                .active_anim()
                .unwrap()
                .looping_sound
                .is_none()
        );
        session.on_puzzle_completed();
        session.handle_click(160, 100);
        assert!(session.active_anim().is_none());
        assert!(!session.audio().is_playing());
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn original_dialogue_is_visible_and_waits_when_muted() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for ega in [false, true] {
        let asset = PuzzleAsset::load(dir, 1, ega).unwrap();
        let sound = asset
            .sounds
            .iter()
            .position(|entry| entry.tag == "OHBOY-01")
            .unwrap();
        let frame = asset.records.iter().position(|record| matches!(record,
        SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
            matches!(command, FrameCommand::Sound { snd_idx: Some(index) } if *index == sound))
    )).unwrap();
        let audio = AudioEngine::new(dir, false, false);
        let mut session = EcsPuzzleHarness::new(1, asset, 2, false, false, false, audio);
        session.set_active_anim(Some(AnimationState {
            seq_idx: 2,
            start_frame: frame,
            previous_frame: frame,
            cur_frame: frame,
            end_frame: frame + 2,
            tick_accum: 0.0,
            frame_delay_secs: 0.0,
            repetitions_left: 1,
            default_delay_secs: 2.0 * DOS_TICK_SECS,
            transition_mode: 0,
            bubble: None,
            wait_for_audio: false,
            looping_sound: None,
        }));
        session.trigger_frame_side_effects(frame);
        let anim = session.active_anim().unwrap();
        assert_eq!(anim.bubble, Some((sound, 0)));
        assert!(anim.wait_for_audio);
        assert_eq!(anim.frame_delay_secs, 20.0 * DOS_TICK_SECS);
        let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
        let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).unwrap();
        let mut renderer = PuzzleRenderer::new();
        let mut fb = Framebuffer::new();
        session.render(&mut renderer, &mut fb, &toolbox, &font, 0, 0);
        assert_eq!(
            fb.pixels[80 * 320 + 10 + SCENE_OFFSET_X as usize],
            if ega { 15 } else { 248 }
        );
        assert_eq!(
            fb.pixels[125 * 320 + 33 + SCENE_OFFSET_X as usize],
            if ega { 1 } else { 16 }
        );
        session.update(1.0);
        assert_eq!(session.active_anim().unwrap().cur_frame, frame);
        session.update(0.2);
        assert_eq!(session.active_anim().unwrap().cur_frame, frame + 1);
        assert_eq!(session.active_anim().unwrap().bubble, None);
        session.meta_mut().text_enabled = false;
        session.trigger_frame_side_effects(frame);
        assert_eq!(session.active_anim().unwrap().bubble, None);
        assert_eq!(
            session.active_anim().unwrap().frame_delay_secs,
            2.0 * DOS_TICK_SECS
        );
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn solved_clicks_select_complete_character_groups() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let audio = AudioEngine::new(dir, false, false);
    let mut session = EcsPuzzleHarness::new(1, asset, 2, false, false, false, audio);
    session.meta_mut().solved = true;
    for (x, y, expected) in [
        (188, 24, "F_GOOFY"),
        (192, 105, "F_GOOFY"),
        (202, 168, "F_GOOFY"),
        (131, 178, "F_MICKEY"),
        (155, 24, "F_ALL"),
        (319, 199, "F_ALL"),
    ] {
        session.set_active_anim(None);
        let display_x = if x < 273 { x + SCENE_OFFSET_X } else { x };
        session.handle_click(display_x, y);
        assert_eq!(session.screen(), AppScreen::Playing);
        let selected = session.active_anim().unwrap().seq_idx;
        assert_eq!(session.meta().asset.sequences[selected].name, expected, "{x},{y}");
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn solved_hotspot_centers_keep_original_selection() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 2, false, false, false);
    let hotspots = session.meta().asset.hotspots.clone();
    for hotspot in &hotspots {
        assert_eq!(
            session.solved_sequence_at(
                hotspot.x as i32 + hotspot.width as i32 / 2,
                hotspot.y as i32 + hotspot.height as i32 / 2,
            ),
            hotspot.playback,
        );
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn all_solved_hotspot_centers_select_and_complete_their_animation() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    let mut checked = 0;
    for ega in [false, true] {
        for id in 1..=15 {
            let asset = PuzzleAsset::load(dir, id, ega).unwrap();
            let toolbox =
                ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega())).unwrap();
            let audio = AudioEngine::new(dir, false, false);
            let mut session = EcsPuzzleHarness::new(id, asset, 2, false, false, false, audio);
            session.meta_mut().solved = true;
            let header = session.meta().asset.sequences[1].header_idx;
            let SceneRecord::SequenceHeader { hotspots, .. } = &session.meta().asset.records[header]
            else {
                panic!("Missing FILM header");
            };
            for hotspot in hotspots.clone() {
                let x = hotspot.x as i32 + hotspot.width as i32 / 2;
                let y = hotspot.y as i32 + hotspot.height as i32 / 2;
                assert_eq!(
                    session.solved_sequence_at(x, y),
                    hotspot.playback,
                    "P{id:02} ega={ega} center={x},{y}"
                );
                session.handle_click(x + SCENE_OFFSET_X, y);
                assert_eq!(session.screen(), AppScreen::Playing);
                assert_eq!(
                    session.active_anim().unwrap().seq_idx,
                    hotspot.playback.seq_idx
                );
                let anim = session.active_anim().unwrap();
                assert_eq!(anim.repetitions_left, hotspot.playback.repetitions.max(1));
                assert_eq!(
                    anim.default_delay_secs,
                    f32::from(hotspot.playback.frame_delay_ticks) * DOS_TICK_SECS
                );
                let mut renderer = PuzzleRenderer::new();
                let mut fb = Framebuffer::new();
                session.render(&mut renderer, &mut fb, &toolbox, &font, x, y);
                for _ in 0..2000 {
                    if session.active_anim().is_none() {
                        break;
                    }
                    session.update(1.0);
                }
                assert!(
                    session.active_anim().is_none(),
                    "P{id:02} ega={ega} sequence={}",
                    hotspot.playback.seq_idx
                );
                session.render(&mut renderer, &mut fb, &toolbox, &font, x, y);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 134);
    println!("{checked} hotspot animations completed across 30 scene selections");
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn tray_scroll_clamps_to_visible_bounds() {
    let asset = PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 1, false).unwrap();
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, false);
    // 3x3 has 9 pieces and 3 visible tray slots -> max scroll = 6.
    assert_eq!(session.meta().visible_tray_slots(), 3);
    session.scroll_tray(-5);
    assert_eq!(session.meta().tray_scroll, 0);
    session.scroll_tray(20);
    assert_eq!(session.meta().tray_scroll, 6);
    session.scroll_tray(-2);
    assert_eq!(session.meta().tray_scroll, 4);

    // When remaining pieces fit in visible slots, scroll stays 0.
    session.truncate_tray(2);
    session.meta_mut().tray_scroll = 0;
    session.scroll_tray(1);
    assert_eq!(session.meta().tray_scroll, 0);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn board_cache_updates_across_toggles_and_placements() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).unwrap();
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, false);
    let mut renderer = PuzzleRenderer::new();

    let mut fb_initial = Framebuffer::new();
    session.render(&mut renderer, &mut fb_initial, &toolbox, &font, 0, 0);

    session.toggle_grid_lines();
    let mut fb_grid = Framebuffer::new();
    session.render(&mut renderer, &mut fb_grid, &toolbox, &font, 0, 0);
    assert_ne!(fb_initial.pixels, fb_grid.pixels);

    session.toggle_toolbox();
    let mut fb_no_tbx = Framebuffer::new();
    session.render(&mut renderer, &mut fb_no_tbx, &toolbox, &font, 0, 0);
    assert_ne!(fb_grid.pixels, fb_no_tbx.pixels);

    session.set_piece_placed(0, true);
    let mut fb_placed = Framebuffer::new();
    session.render(&mut renderer, &mut fb_placed, &toolbox, &font, 0, 0);
    assert_ne!(fb_no_tbx.pixels, fb_placed.pixels);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn render_reference_matches_scene_view() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for ega in [false, true] {
        let asset = PuzzleAsset::load(dir, 1, ega).unwrap();
        let session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, false);
        let mut renderer = PuzzleRenderer::new();
        let mut expected = Framebuffer::new();
        render_scene_view(
            &mut expected,
            &session.meta().asset,
            session.meta().asset.static_frame_idx(),
        );
        let mut actual = Framebuffer::new();
        renderer.render_reference(session.world(), &mut actual);
        assert_eq!(actual.pixels, expected.pixels, "ega={ega}");
        assert_eq!(actual.palette, expected.palette, "ega={ega}");
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn update_ignores_non_finite_and_negative_dt() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, true);
    session.meta_mut().elapsed_secs = 12.5;
    let first_e = session
        .world_mut()
        .query::<(Entity, &PieceIdentity)>()
        .iter(session.world())
        .find_map(|(e, id)| (id.index == 0).then_some(e))
        .unwrap();
    session
        .world_mut()
        .entity_mut(first_e)
        .insert(HintHighlight::new(1.0));
    session.meta_mut().set_status_message("OK", 2.0);

    for bad_dt in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        session.update(bad_dt);
        assert_eq!(session.meta().elapsed_secs, 12.5);
        assert_eq!(session.hint_piece(), Some((0, 1.0)));
        assert_eq!(
            session
                .meta()
                .status_message
                .as_ref()
                .unwrap()
                .1
                .remaining_secs(),
            2.0
        );
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn scroll_tray_handles_extreme_deltas() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, false);
    session.scroll_tray(i32::MAX);
    assert_eq!(session.meta().tray_scroll, 6);
    session.scroll_tray(i32::MIN);
    assert_eq!(session.meta().tray_scroll, 0);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn replace_scene_asset_preserves_solved_and_partial_state() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let vga = PuzzleAsset::load(dir, 1, false).unwrap();
    let ega = PuzzleAsset::load(dir, 1, true).unwrap();
    let mut session = EcsPuzzleHarness::new_no_audio(1, vga, 3, true, true, true);
    session.set_piece_placed(0, true);
    // Pick up the first remaining tray piece
    let held = session.tray_order()[0];
    let held_e = session
        .world_mut()
        .query::<(Entity, &PieceIdentity)>()
        .iter(session.world())
        .find_map(|(e, id)| (id.index == held).then_some(e))
        .unwrap();
    *session.world_mut().get_mut::<PieceLocation>(held_e).unwrap() = PieceLocation::Held {
        is_anchor: true,
        group_offset: (0, 0),
        group_order: 0,
    };
    session.meta_mut().tray_scroll = 2;
    session.meta_mut().elapsed_secs = 42.0;

    let expected_tray = session.tray_order();
    session.replace_scene_asset(ega);
    assert!(session.meta().asset.uses_ega());
    assert!(session.is_placed(0));
    assert_eq!(session.held_piece(), Some(held));
    assert_eq!(session.tray_order(), expected_tray);
    assert_eq!(session.meta().tray_scroll, 2);
    assert_eq!(session.meta().elapsed_secs, 42.0);
    assert!(!session.meta().solved);

    let vga2 = PuzzleAsset::load(dir, 1, false).unwrap();
    session.step(
        AppInput {
            f2: true,
            ..Default::default()
        },
        0.0,
    );
    assert!(session.meta().solved);
    session.replace_scene_asset(vga2);
    assert!(session.meta().solved);
    assert!(!session.meta().toolbox_open);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn blend_picture_guide_matches_reference_with_placed_held_and_clipped_pieces() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        let asset = PuzzleAsset::load(dir, 1, ega).unwrap();
        let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(ega)).unwrap();
        let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, false);
        let mut renderer = PuzzleRenderer::new();
        session.set_piece_placed(0, true);
        session.set_piece_placed(4, true);
        let held_e = session
            .world_mut()
            .query::<(Entity, &PieceIdentity)>()
            .iter(session.world())
            .find_map(|(e, id)| (id.index == 1).then_some(e))
            .unwrap();
        *session.world_mut().get_mut::<PieceLocation>(held_e).unwrap() = PieceLocation::Held {
            is_anchor: true,
            group_offset: (0, 0),
            group_order: 0,
        };
        session.step(AppInput::default(), 0.0);

        let pfx = session.meta().playfield_x();
        let pfy = session.meta().playfield_y();
        let cursor_positions = [
            (160, 100),
            (pfx - 3, pfy - 3),
            (pfx + PLAYFIELD_W - 2, pfy + PLAYFIELD_H - 2),
            (2, 2),
            (318, 198),
        ];

        let pal = session.meta().asset.palette;
        let bg_idx = session.meta().full_picture_fb.color_index([204, 204, 187]) as usize;
        let bg_rgba = [pal[bg_idx][0], pal[bg_idx][1], pal[bg_idx][2], 255];

        for &(mx, my) in &cursor_positions {
            let mut fb = Framebuffer::new();
            session.render(&mut renderer, &mut fb, &toolbox, &font, mx, my);

            let mut actual = vec![0u8; 320 * 200 * 4];
            fb.to_rgba8(&mut actual);
            let mut expected = actual.clone();

            renderer.blend_picture_guide(session.world_mut(), &mut actual, mx, my);

            let count = session.piece_count();
            let mut placed_pieces = Vec::new();
            for i in 0..count {
                if session.is_placed(i) {
                    placed_pieces.push((session.piece_geom(i), session.piece_sprite(i)));
                }
            }
            let held_geom = session.piece_geom(1);
            let held_sprite = session.piece_sprite(1);

            // Simple pixel-by-pixel reference implementation
            for py in 0..PLAYFIELD_H {
                for px in 0..PLAYFIELD_W {
                    let sx = pfx + px;
                    let sy = pfy + py;
                    if !(0..320).contains(&sx) || !(0..200).contains(&sy) {
                        continue;
                    }
                    let placed_here = placed_pieces.iter().any(|(piece, sprite)| {
                        let dx = px - piece.target_x;
                        let dy = py - piece.target_y;
                        if !(0..i32::from(sprite.width)).contains(&dx)
                            || !(0..i32::from(sprite.sub_h)).contains(&dy)
                        {
                            return false;
                        }
                        let src = (dy + i32::from(sprite.sub_y)) as usize
                            * usize::from(sprite.stride)
                            + (dx + i32::from(sprite.sub_x)) as usize;
                        sprite.is_opaque(src)
                    });
                    if placed_here {
                        continue;
                    }

                    let held_here = {
                        let hx = mx - held_geom.base_w / 2 + held_geom.target_x - held_geom.base_x - pfx;
                        let hy = my - held_geom.base_h / 2 + held_geom.target_y - held_geom.base_y - pfy;
                        let dx = px - hx;
                        let dy = py - hy;
                        if !(0..i32::from(held_sprite.width)).contains(&dx)
                            || !(0..i32::from(held_sprite.sub_h)).contains(&dy)
                        {
                            false
                        } else {
                            let src = (dy + i32::from(held_sprite.sub_y)) as usize
                                * usize::from(held_sprite.stride)
                                + (dx + i32::from(held_sprite.sub_x)) as usize;
                            held_sprite.is_opaque(src)
                        }
                    };
                    if held_here {
                        continue;
                    }

                    let off = ((sy * 320 + sx) * 4) as usize;
                    if expected[off..off + 4] != bg_rgba {
                        continue;
                    }
                    let pic_c = session.meta().full_picture_fb.get_pixel(2 + px, 2 + py);
                    let pic_rgb = session.meta().full_picture_fb.palette[pic_c as usize];
                    for ch in 0..3 {
                        expected[off + ch] =
                            ((u16::from(bg_rgba[ch]) * 4 + u16::from(pic_rgb[ch])) / 5) as u8;
                    }
                    expected[off + 3] = 255;
                }
            }

            assert_eq!(actual, expected, "ega={ega}, cursor=({mx}, {my})");
        }
    }
}

fn synthetic_puzzle_asset(id: usize, ega: bool) -> PuzzleAsset {
    use crate::assets::{NamedSequence, SequencePlayback};
    let mut palette = [[0u8; 3]; 256];
    palette[1] = [0, 0, 170];
    palette[2] = [0, 170, 0];
    palette[4] = [170, 0, 0];
    palette[7] = [204, 204, 187];
    palette[8] = [85, 85, 85];
    palette[14] = [255, 255, 85];
    palette[15] = [255, 255, 255];
    let sprite = Sprite8061 {
        stride: 274,
        height: 200,
        transparent_color: 0,
        bpp: if ega { 4 } else { 8 },
        sub_x: 0,
        sub_y: 0,
        width: 274,
        sub_h: 200,
        pixels: vec![if ega { 2 } else { 1 }; 274 * 200],
        opacity: None,
    };
    let records = vec![
        SceneRecord::Frame {
            bg_color: 0,
            bg_frame_idx: 0,
            commands: vec![FrameCommand::Sprite {
                x: 0,
                y: 0,
                sprite_idx: 0,
                flags: 0,
            }],
        },
        SceneRecord::SequenceHeader {
            name: "FILM".to_string(),
            default_playback: Some(SequencePlayback {
                seq_idx: 0,
                repetitions: 1,
                frame_delay_ticks: 2,
            }),
            hotspots: Vec::new(),
        },
        SceneRecord::Frame {
            bg_color: 0,
            bg_frame_idx: 0,
            commands: vec![
                FrameCommand::Delay { ticks: 2, raw: 8 },
                FrameCommand::Sprite {
                    x: 0,
                    y: 0,
                    sprite_idx: 0,
                    flags: 0,
                },
            ],
        },
        SceneRecord::Frame {
            bg_color: 0,
            bg_frame_idx: 0,
            commands: vec![FrameCommand::Sprite {
                x: 0,
                y: 0,
                sprite_idx: 0,
                flags: 0,
            }],
        },
    ];
    PuzzleAsset {
        id,
        palette,
        records,
        sequences: vec![NamedSequence {
            name: "FILM".to_string(),
            header_idx: 1,
            start_frame: 2,
            end_frame: 4,
        }],
        hotspots: Vec::new(),
        full_anim_seq_idx: 0,
        sounds: Vec::new(),
        sprites: vec![sprite],
    }
}

fn synthetic_toolbox_and_font() -> (ToolboxAsset, FontAsset) {
    let mut palette = [[0u8; 3]; 256];
    palette[15] = [255, 255, 255];
    let tbx = ToolboxAsset {
        palette,
        sprites: Vec::new(),
    };
    let font = FontAsset {
        name: "TEST".to_string(),
        strip_width: 96,
        height: 8,
        stride_bytes: 12,
        char_x: [0; 96],
        char_w: [4; 96],
        bitmap: vec![0xAA; 8 * 12],
    };
    (tbx, font)
}

#[test]
fn pure_gameplay_pick_place_hint_and_complete_without_assets() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 2, true, false, true);

    assert_eq!(session.piece_count(), 4);
    assert_eq!(session.tray_order().len(), 4);
    assert!(!session.meta().solved);

    // Trigger hint and verify blink phase flips over time
    session.trigger_hint();
    let hint_key_0 = session.present_key(false, 120, 80);
    session.update(0.125);
    let hint_key_1 = session.present_key(false, 120, 80);
    assert_ne!(hint_key_0, hint_key_1);

    // Pick and place all 4 pieces
    for _ in 0..4 {
        session.handle_pointer_press(20, 40);
        let held = session.held_piece().expect("piece picked from tray");
        let piece = session.piece_geom(held);
        let tx = session.meta().playfield_x() + piece.base_x + piece.base_w / 2;
        let ty = session.meta().playfield_y() + piece.base_y + piece.base_h / 2;
        session.handle_pointer_press(tx, ty);
        assert!(session.held_piece().is_none());
    }

    assert!(session.meta().solved);
    assert!(!session.meta().toolbox_open);
    assert!(session.active_anim().is_some());

    // Clicking during completion animation skips it
    session.handle_pointer_press(160, 100);
    assert!(session.active_anim().is_none());

    // Clicking top-left menu hotspot opens the in-game menu
    session.handle_pointer_press(5, 5);
    assert!(session.world().resource::<InGameMenuState>().open);
}

#[test]
fn present_key_tracks_idle_hover_guide_timer_and_dissolve_without_assets() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, true);

    // Idle board pointer motion does not invalidate
    let k0 = session.present_key(true, 120, 80);
    let k1 = session.present_key(true, 200, 140);
    assert_eq!(k0, k1);

    // Timer ticks do not invalidate the DOS framebuffer key
    session.update(1.5);
    assert!(session.meta().elapsed_secs >= 1.5);
    assert_eq!(session.present_key(true, 120, 80), k0);

    // Tray hover invalidates (and keyboard pointer tray hover also invalidates)
    let k_hover = session.present_key(true, 20, 40);
    assert_ne!(k0, k_hover);
    session.move_keyboard_pointer(-1.0, 0.0, 0.0);
    let k_kbd_hover = session.present_key(true, 200, 140);
    assert_ne!(k0, k_kbd_hover);
    session.meta_mut().keyboard_pointer = None;
    let k_after_clear = session.present_key(true, 120, 80);
    assert_ne!(k_kbd_hover, k_after_clear);
    assert_eq!(session.present_key(true, 200, 140), k_after_clear);
    let k0 = k_after_clear;

    // Picture guide toggle invalidates
    assert_ne!(session.present_key(false, 120, 80), k0);

    // Held piece tracks movement
    session.handle_pointer_press(20, 40);
    let k_held_a = session.present_key(true, 100, 100);
    let k_held_b = session.present_key(true, 104, 100);
    assert_ne!(k_held_a, k_held_b);

    // Auto-solve starts dissolve animation and dissolve steps advance key
    session.step(
        AppInput {
            f2: true,
            ..Default::default()
        },
        0.0,
    );
    let k_anim_0 = session.present_key(true, 0, 0);
    session.update(0.05);
    let k_anim_1 = session.present_key(true, 0, 0);
    assert_ne!(k_anim_0, k_anim_1);
}

#[test]
fn puzzle_renderer_cache_invalidates_on_board_and_video_mode_changes_without_assets() {
    let (tbx, font) = synthetic_toolbox_and_font();
    let vga = synthetic_puzzle_asset(1, false);
    let mut ega = synthetic_puzzle_asset(1, true);
    ega.palette[1] = [0, 85, 255];
    let mut session = EcsPuzzleHarness::new_no_audio(1, vga, 2, true, false, false);
    let mut renderer = PuzzleRenderer::new();

    let mut fb_vga = Framebuffer::new();
    session.render(&mut renderer, &mut fb_vga, &tbx, &font, 160, 100);

    // Grid lines toggle updates cached board
    session.toggle_grid_lines();
    let mut fb_grid = Framebuffer::new();
    session.render(&mut renderer, &mut fb_grid, &tbx, &font, 160, 100);
    assert_ne!(fb_vga.pixels, fb_grid.pixels);

    // Video mode switch via replace_scene_asset invalidates board & guide caches
    session.replace_scene_asset(ega);
    let mut fb_ega = Framebuffer::new();
    session.render(&mut renderer, &mut fb_ega, &tbx, &font, 160, 100);
    assert_ne!(fb_grid.pixels, fb_ega.pixels);
}

#[test]
fn puzzle_renderer_isolates_caches_across_replaced_sessions_without_assets() {
    let (tbx, font) = synthetic_toolbox_and_font();
    let vga = synthetic_puzzle_asset(1, false);
    let mut ega = synthetic_puzzle_asset(1, true);
    ega.palette[1] = [0, 85, 255];

    let mut session_a = EcsPuzzleHarness::new_no_audio(1, vga.clone(), 2, false, false, false);
    let mut session_b = EcsPuzzleHarness::new_no_audio(1, vga, 4, true, true, false);
    let mut session_c = EcsPuzzleHarness::new_no_audio(1, ega, 2, false, false, false);

    assert_ne!(
        session_a.present_key(true, 120, 80),
        session_b.present_key(true, 120, 80)
    );
    assert_ne!(
        session_a.present_key(true, 120, 80),
        session_c.present_key(true, 120, 80)
    );

    let mut reused = PuzzleRenderer::new();

    let mut fb_a = Framebuffer::new();
    session_a.render(&mut reused, &mut fb_a, &tbx, &font, 160, 100);
    let mut rgba_a = vec![0u8; 320 * 200 * 4];
    fb_a.to_rgba8(&mut rgba_a);
    reused.blend_picture_guide(&mut session_a.world_mut(), &mut rgba_a, 160, 100);

    // Same puzzle_num and initial revision counters, different grid settings
    let mut fb_b_reused = Framebuffer::new();
    session_b.render(&mut reused, &mut fb_b_reused, &tbx, &font, 160, 100);
    let mut fb_b_fresh = Framebuffer::new();
    let mut fresh_b = PuzzleRenderer::new();
    session_b.render(&mut fresh_b, &mut fb_b_fresh, &tbx, &font, 160, 100);
    assert_eq!(fb_b_reused.pixels, fb_b_fresh.pixels);
    assert_eq!(fb_b_reused.palette, fb_b_fresh.palette);
    assert_ne!(fb_b_reused.pixels, fb_a.pixels);

    // Same puzzle_num, same grid settings, different video mode (EGA)
    let mut fb_c_reused = Framebuffer::new();
    session_c.render(&mut reused, &mut fb_c_reused, &tbx, &font, 160, 100);
    let mut rgba_c_reused = vec![0u8; 320 * 200 * 4];
    fb_c_reused.to_rgba8(&mut rgba_c_reused);
    reused.blend_picture_guide(&mut session_c.world_mut(), &mut rgba_c_reused, 160, 100);

    let mut fresh_c = PuzzleRenderer::new();
    let mut fb_c_fresh = Framebuffer::new();
    session_c.render(&mut fresh_c, &mut fb_c_fresh, &tbx, &font, 160, 100);
    let mut rgba_c_fresh = vec![0u8; 320 * 200 * 4];
    fb_c_fresh.to_rgba8(&mut rgba_c_fresh);
    fresh_c.blend_picture_guide(&mut session_c.world_mut(), &mut rgba_c_fresh, 160, 100);

    assert_eq!(fb_c_reused.pixels, fb_c_fresh.pixels);
    assert_eq!(fb_c_reused.palette, fb_c_fresh.palette);
    assert_eq!(rgba_c_reused, rgba_c_fresh);
    assert_ne!(fb_c_reused.palette, fb_a.palette);
    assert_ne!(rgba_c_reused, rgba_a);
}

#[test]
fn curved_cut_pieces_have_no_horizontal_bridge_or_gap_without_assets() {
    let asset = synthetic_puzzle_asset(1, false);
    for dim in 2..=8 {
        let mut session = EcsPuzzleHarness::new_no_audio(1, asset.clone(), dim, true, false, false);
        let mut coverage = vec![0u8; 270 * 196];
        let count = session.piece_count();
        for idx in 0..count {
            let ident = session.piece_identity(idx);
            let p = session.piece_geom(idx);
            let sprite = session.piece_sprite(idx);
            let w = sprite.width as usize;
            let h = sprite.sub_h as usize;

            // 1. Every piece must cover its exact pixels once and form a single 4-connected shape.
            let mut total_opaque = 0usize;
            for dy in 0..h {
                for dx in 0..w {
                    let i = dy * w + dx;
                    if sprite.is_opaque(i) {
                        total_opaque += 1;
                        let px = p.target_x + dx as i32;
                        let py = p.target_y + dy as i32;
                        coverage[(py * 270 + px) as usize] += 1;
                    }
                }
            }
            let start = (0..w * h).find(|&i| sprite.is_opaque(i)).unwrap();
            let mut seen = vec![false; w * h];
            let mut stack = vec![start];
            seen[start] = true;
            let mut reached = 0usize;
            while let Some(cur) = stack.pop() {
                reached += 1;
                let x = cur % w;
                let y = cur / w;
                for (nx, ny) in [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ] {
                    if nx < w && ny < h {
                        let ni = ny * w + nx;
                        if !seen[ni] && sprite.is_opaque(ni) {
                            seen[ni] = true;
                            stack.push(ni);
                        }
                    }
                }
            }
            assert_eq!(
                reached, total_opaque,
                "dim={dim} piece {idx} (r={}, c={}) has disconnected pixels ({reached} != {total_opaque})",
                ident.home_row, ident.home_col
            );

            // 2. A downward blank on row r > 0 must not leave a 1px horizontal bridge at y = base_y.
            if ident.home_row > 0 {
                let top_row = (p.base_y - p.target_y) as usize;
                let has_blank_below = (0..p.base_w as usize).any(|dx| {
                    let lx = (p.base_x - p.target_x) as usize + dx;
                    !sprite.is_opaque((top_row + 1) * w + lx)
                });
                if has_blank_below {
                    let open_at_seam = (0..p.base_w as usize).any(|dx| {
                        let lx = (p.base_x - p.target_x) as usize + dx;
                        !sprite.is_opaque(top_row * w + lx)
                    });
                    assert!(
                        open_at_seam,
                        "dim={dim} piece {idx} (r={}, c={}) has a 1px horizontal bridge across its top blank",
                        ident.home_row, ident.home_col
                    );
                }
            }
        }
        assert!(coverage.iter().all(|&n| n == 1));
    }
}

#[test]
fn curved_cut_pieces_match_framebuffer_rasterization_without_assets() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut ref_fb = Framebuffer::new();
    for dim in 2..=8 {
        let mut session = EcsPuzzleHarness::new_no_audio(1, asset.clone(), dim, true, false, false);
        let col_w = grid_col_widths(dim);
        let row_h = grid_row_heights(dim);
        let mut col_x = vec![0i32; dim + 1];
        for c in 0..dim {
            col_x[c + 1] = col_x[c] + col_w[c];
        }
        let mut row_y = vec![0i32; dim + 1];
        for r in 0..dim {
            row_y[r + 1] = row_y[r] + row_h[r];
        }
        let mut rng = LcgRng::new(1103515245u32.wrapping_add((dim as u32) * 12345));
        let mut next_edge = || {
            let code = rng.next() & 7;
            (code & 3, if code < 4 { 1 } else { -1 })
        };
        let mut v_edges = vec![vec![(0usize, 1i32); dim + 1]; dim];
        for row in &mut v_edges {
            for edge in row.iter_mut().take(dim).skip(1) {
                *edge = next_edge();
            }
        }
        let mut h_edges = vec![vec![(0usize, 1i32); dim]; dim + 1];
        for row in h_edges.iter_mut().take(dim).skip(1) {
            for edge in row {
                *edge = next_edge();
            }
        }

        let count = session.piece_count();
        for idx in 0..count {
            let ident = session.piece_identity(idx);
            let p = session.piece_geom(idx);
            let sprite = session.piece_sprite(idx);
            let (r, c) = (ident.home_row, ident.home_col);
            let (bx, by, bw, bh) = (col_x[c], row_y[r], col_w[c], row_h[r]);
            let borders = [
                (c > 0, true, v_edges[r][c], bx, by, -1),
                (c + 1 < dim, true, v_edges[r][c + 1], bx + bw, by, 1),
                (r > 0, false, h_edges[r][c], bx, by, -1),
                (r + 1 < dim, false, h_edges[r + 1][c], bx, by + bh, 1),
            ];
            ref_fb.clear(0);
            ref_fb.fill_rect(bx, by, bw, bh, 1);
            for &(active, is_vert, (var, dir), ex, ey, out_dir) in &borders {
                if !active {
                    continue;
                }
                let span = if is_vert { bh } else { bw };
                let (_, pts) = edge_curve(is_vert, span, var);
                let poly: Vec<(i32, i32)> = pts
                    .iter()
                    .map(|&(dx, dy)| {
                        if is_vert {
                            (ex + i32::from(dx) * dir, ey + i32::from(dy))
                        } else {
                            (ex + i32::from(dx), ey + i32::from(dy) * dir)
                        }
                    })
                    .collect();
                let fill_val = u8::from(dir == out_dir);
                ref_fb.fill_polygon(&poly, fill_val);
                if !is_vert && dir == 1 {
                    let (mut x_min, mut x_max) = (i32::MAX, i32::MIN);
                    for &(px, py) in &poly {
                        if py == ey {
                            x_min = x_min.min(px);
                            x_max = x_max.max(px);
                        }
                    }
                    if x_min <= x_max {
                        ref_fb.fill_rect(x_min, ey, x_max - x_min + 1, 1, fill_val);
                    }
                }
            }
            let w = sprite.width as usize;
            let h = sprite.sub_h as usize;
            let opacity = sprite.opacity.as_ref().unwrap();
            for dy in 0..h {
                for dx in 0..w {
                    let expected =
                        ref_fb.get_pixel(p.target_x + dx as i32, p.target_y + dy as i32) != 0;
                    assert_eq!(
                        opacity[dy * w + dx],
                        expected,
                        "dim={dim} piece {idx} ({r},{c}) mismatch at ({dx},{dy})"
                    );
                }
            }
        }
    }
}

#[test]
fn solved_sequence_at_indexes_hotspots_by_subframe_order() {
    use crate::assets::{Hotspot, NamedSequence};

    let mut palette = [[0u8; 3]; 256];
    palette[7] = [204, 204, 187];
    let default_playback = SequencePlayback {
        seq_idx: 1,
        repetitions: 1,
        frame_delay_ticks: 2,
    };
    let hs0 = Hotspot {
        playback: SequencePlayback {
            seq_idx: 2,
            repetitions: 1,
            frame_delay_ticks: 4,
        },
        x: 10,
        y: 10,
        width: 20,
        height: 20,
    };
    let hs1 = Hotspot {
        playback: SequencePlayback {
            seq_idx: 3,
            repetitions: 2,
            frame_delay_ticks: 6,
        },
        x: 60,
        y: 10,
        width: 20,
        height: 20,
    };
    let asset = PuzzleAsset {
        id: 1,
        palette,
        records: vec![
            SceneRecord::SequenceHeader {
                name: "INIT".to_string(),
                default_playback: None,
                hotspots: Vec::new(),
            },
            SceneRecord::SequenceHeader {
                name: "FILM".to_string(),
                default_playback: Some(default_playback),
                hotspots: vec![hs0.clone(), hs1.clone()],
            },
            // FILM start frame with non-SubFrame commands preceding SubFrames
            SceneRecord::Frame {
                bg_frame_idx: 0,
                bg_color: 0,
                commands: vec![
                    FrameCommand::Delay { raw: 0, ticks: 0 },
                    FrameCommand::Sound { snd_idx: None },
                    FrameCommand::SubFrame {
                        dx: 0,
                        dy: 0,
                        frame_idx: 3,
                        flags: 0,
                    },
                    FrameCommand::SubFrame {
                        dx: 0,
                        dy: 0,
                        frame_idx: 4,
                        flags: 0,
                    },
                ],
            },
            SceneRecord::Frame {
                bg_frame_idx: 0,
                bg_color: 0,
                commands: vec![],
            },
            SceneRecord::Frame {
                bg_frame_idx: 0,
                bg_color: 0,
                commands: vec![],
            },
        ],
        sequences: vec![
            NamedSequence {
                name: "INIT".to_string(),
                header_idx: 0,
                start_frame: 1,
                end_frame: 1,
            },
            NamedSequence {
                name: "FILM".to_string(),
                header_idx: 1,
                start_frame: 2,
                end_frame: 5,
            },
        ],
        hotspots: vec![hs0.clone(), hs1.clone()],
        full_anim_seq_idx: 1,
        sounds: Vec::new(),
        sprites: Vec::new(),
    };
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 2, false, false, false);
    assert_eq!(session.solved_sequence_at(20, 20), hs0.playback);
    assert_eq!(session.solved_sequence_at(70, 20), hs1.playback);
}

#[test]
fn blend_picture_guide_skips_active_animation() {
    let mut palette = [[0u8; 3]; 256];
    palette[7] = [204, 204, 187];
    let asset = PuzzleAsset {
        id: 1,
        palette,
        records: vec![SceneRecord::Frame {
            bg_frame_idx: 0,
            bg_color: 1,
            commands: vec![],
        }],
        sequences: Vec::new(),
        hotspots: Vec::new(),
        full_anim_seq_idx: 0,
        sounds: Vec::new(),
        sprites: Vec::new(),
    };
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 2, false, false, false);
    session.set_active_anim(Some(AnimationState {
        seq_idx: 0,
        start_frame: 0,
        previous_frame: 0,
        cur_frame: 0,
        end_frame: 1,
        tick_accum: 0.0,
        frame_delay_secs: 0.1,
        repetitions_left: 1,
        default_delay_secs: 0.1,
        transition_mode: 0,
        bubble: None,
        wait_for_audio: false,
        looping_sound: None,
    }));
    let mut renderer = PuzzleRenderer::new();
    let mut rgba = [204u8, 204, 187, 255].repeat(320 * 200);
    let expected = rgba.clone();
    renderer.blend_picture_guide(session.world_mut(), &mut rgba, 160, 100);
    assert_eq!(rgba, expected);
}

#[test]
fn blit_thumbnail_ignores_zero_dimensions() {
    let mut fb = Framebuffer::new();
    let spr = Sprite8061::from_8bpp(0, 0, 0, Vec::new(), Vec::new());
    fb.blit_thumbnail(&spr, 10, 10, 20, 20);
    let non_zero = Sprite8061::from_8bpp(4, 4, 0, vec![5; 16], vec![true; 16]);
    fb.blit_thumbnail(&non_zero, 10, 10, 0, 20);
    fb.blit_thumbnail(&non_zero, 10, 10, 20, 0);
    fb.blit_thumbnail(&non_zero, 10, 10, -5, 20);
    fb.blit_thumbnail(&non_zero, 10, 10, 20, -5);
    assert!(fb.pixels.iter().all(|&p| p == 0));
}

#[test]
fn replace_scene_asset_clears_hint_when_piece_count_changes() {
    let mut asset_two = synthetic_puzzle_asset(1, false);
    asset_two.records.push(SceneRecord::Frame {
        bg_color: 0,
        bg_frame_idx: 0,
        commands: vec![
            FrameCommand::SubFrame {
                dx: 0,
                dy: 0,
                frame_idx: 2,
                flags: 0,
            },
            FrameCommand::SubFrame {
                dx: 10,
                dy: 10,
                frame_idx: 3,
                flags: 0,
            },
        ],
    });
    let mut asset_one = synthetic_puzzle_asset(1, true);
    asset_one.records.push(SceneRecord::Frame {
        bg_color: 0,
        bg_frame_idx: 0,
        commands: vec![FrameCommand::SubFrame {
            dx: 0,
            dy: 0,
            frame_idx: 2,
            flags: 0,
        }],
    });
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset_two, 0, true, false, false);
    assert_eq!(session.piece_count(), 2);
    let hint_e = session
        .world_mut()
        .query::<(Entity, &PieceIdentity)>()
        .iter(session.world())
        .find_map(|(e, id)| (id.index == 1).then_some(e))
        .unwrap();
    session
        .world_mut()
        .entity_mut(hint_e)
        .insert(HintHighlight::new(1.5));

    session.replace_scene_asset(asset_one);
    assert_eq!(session.piece_count(), 1);
    assert_eq!(session.hint_piece(), None);
}

#[test]
fn hidden_tray_ignores_scroll() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, false);

    session.scroll_tray(1);
    assert_eq!(session.meta().tray_scroll, 1);
    session.toggle_toolbox();
    assert!(!session.meta().toolbox_open);

    // Scrolling while toolbox is hidden must not mutate tray_scroll.
    session.scroll_tray(-1);
    assert_eq!(session.meta().tray_scroll, 1);

    // Solving also hides the tray and ignores scroll.
    session.toggle_toolbox();
    session.step(
        AppInput {
            f2: true,
            ..Default::default()
        },
        0.0,
    );
    assert!(session.meta().solved);
    session.scroll_tray(1);
    assert_eq!(session.meta().tray_scroll, 0);
}

#[test]
fn replace_scene_asset_reopens_toolbox_when_solved_piece_count_changes() {
    let mut asset_two = synthetic_puzzle_asset(1, false);
    asset_two.records.push(SceneRecord::Frame {
        bg_color: 0,
        bg_frame_idx: 0,
        commands: vec![
            FrameCommand::SubFrame {
                dx: 0,
                dy: 0,
                frame_idx: 2,
                flags: 0,
            },
            FrameCommand::SubFrame {
                dx: 10,
                dy: 10,
                frame_idx: 3,
                flags: 0,
            },
        ],
    });
    let mut asset_one = synthetic_puzzle_asset(1, true);
    asset_one.records.push(SceneRecord::Frame {
        bg_color: 0,
        bg_frame_idx: 0,
        commands: vec![FrameCommand::SubFrame {
            dx: 0,
            dy: 0,
            frame_idx: 2,
            flags: 0,
        }],
    });
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset_two, 0, true, false, false);
    session.step(
        AppInput {
            f2: true,
            ..Default::default()
        },
        0.0,
    );
    assert!(session.meta().solved);
    assert!(!session.meta().toolbox_open);

    session.replace_scene_asset(asset_one);
    assert!(!session.meta().solved);
    assert!(session.meta().toolbox_open);
}

#[test]
fn cut_pieces_all_grids_cover_playfield_without_gaps_overlaps_or_trans_leaks() {
    let mut asset = synthetic_puzzle_asset(1, false);
    // Include `0xF8` (cut_pieces sentinel) and varied colors across the 270x196 playfield.
    let mut cmds = vec![FrameCommand::Rect {
        x: 2,
        y: 2,
        width: PLAYFIELD_W as i16,
        height: PLAYFIELD_H as i16,
        color: 0x00F8,
        flags: 0,
    }];
    for band in 0..7u16 {
        cmds.push(FrameCommand::Rect {
            x: 10 + (band as i16) * 35,
            y: 12 + (band as i16) * 20,
            width: 28,
            height: 24,
            color: 0x0010 + band * 17,
            flags: 0,
        });
    }
    asset.records[1] = SceneRecord::Frame {
        bg_color: 0,
        bg_frame_idx: 0,
        commands: cmds,
    };

    let (tbx, font) = synthetic_toolbox_and_font();
    let mut renderer = PuzzleRenderer::new();
    let mut fb = Framebuffer::new();

    for puzzle_num in [1usize, 7, 15] {
        for dim in 2..=8 {
            for curved in [false, true] {
                let mut session = EcsPuzzleHarness::new_no_audio(
                    puzzle_num,
                    asset.clone(),
                    dim,
                    curved,
                    false,
                    false,
                );
                let mut coverage = vec![0u8; (PLAYFIELD_W * PLAYFIELD_H) as usize];

                let count = session.piece_count();
                for idx in 0..count {
                    let piece = session.piece_geom(idx);
                    let sprite = session.piece_sprite(idx);
                    let w = sprite.width as i32;
                    let h = sprite.sub_h as i32;
                    let opacity = sprite.opacity.as_ref().expect("cut piece opacity mask");
                    assert_eq!(opacity.len(), (w * h) as usize);
                    for y in 0..h {
                        for x in 0..w {
                            let i = (y * w + x) as usize;
                            if opacity[i] {
                                let px = piece.target_x + x;
                                let py = piece.target_y + y;
                                assert!(
                                    (0..PLAYFIELD_W).contains(&px)
                                        && (0..PLAYFIELD_H).contains(&py),
                                    "out of bounds puzzle={puzzle_num} dim={dim} curved={curved} ({px}, {py})"
                                );
                                coverage[(py * PLAYFIELD_W + px) as usize] += 1;
                                assert_eq!(
                                    sprite.pixels[i],
                                    session.meta().full_picture_fb.get_pixel(2 + px, 2 + py),
                                    "pixel mismatch puzzle={puzzle_num} dim={dim} curved={curved} ({px}, {py})"
                                );
                            } else {
                                assert_eq!(sprite.pixels[i], 0xF8);
                            }
                        }
                    }
                }

                assert!(
                    coverage.iter().all(|&n| n == 1),
                    "puzzle={puzzle_num} dim={dim} curved={curved}: missing={} overlapping={}",
                    coverage.iter().filter(|&&n| n == 0).count(),
                    coverage.iter().filter(|&&n| n > 1).count()
                );

                // Verify placing all pieces on the board reconstructs the exact 270x196 interior.
                session.set_all_placed();
                session.render(&mut renderer, &mut fb, &tbx, &font, 160, 100);
                let pfx = session.meta().playfield_x();
                let pfy = session.meta().playfield_y();
                for y in 0..PLAYFIELD_H {
                    for x in 0..PLAYFIELD_W {
                        assert_eq!(
                            fb.get_pixel(pfx + x, pfy + y),
                            session.meta().full_picture_fb.get_pixel(2 + x, 2 + y),
                            "board mismatch puzzle={puzzle_num} dim={dim} curved={curved} ({x}, {y})"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn toolbox_cache_and_guide_match_uncached_render_across_interactions() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 4, true, true, false);
    let (tbx, font) = synthetic_toolbox_and_font();

    session.set_piece_placed(0, true);
    session.set_piece_placed(5, true);
    let held_e = session
        .world_mut()
        .query::<(Entity, &PieceIdentity)>()
        .iter(session.world())
        .find_map(|(e, id)| (id.index == 3).then_some(e))
        .unwrap();
    *session.world_mut().get_mut::<PieceLocation>(held_e).unwrap() = PieceLocation::Held {
        is_anchor: true,
        group_offset: (0, 0),
        group_order: 0,
    };
    session.step(AppInput::default(), 0.0);

    let mut cached_renderer = PuzzleRenderer::new();
    let mut fb_cached = Framebuffer::new();
    let mut fb_uncached = Framebuffer::new();
    let mut rgba_cached = vec![0u8; 320 * 200 * 4];
    let mut rgba_uncached = vec![0u8; 320 * 200 * 4];

    let steps = [
        (20, 40, 0),
        (20, 90, 0),
        (160, 100, 0),
        (20, 140, 2),
        (120, 75, 2),
        (55, 15, 2),
    ];

    for (mx, my, scroll) in steps {
        let delta = scroll - session.meta().tray_scroll as i32;
        session.scroll_tray(delta);
        session.render(&mut cached_renderer, &mut fb_cached, &tbx, &font, mx, my);
        fb_cached.to_rgba8(&mut rgba_cached);
        cached_renderer.blend_picture_guide(session.world_mut(), &mut rgba_cached, mx, my);

        let mut fresh = PuzzleRenderer::new();
        session.render(&mut fresh, &mut fb_uncached, &tbx, &font, mx, my);
        fb_uncached.to_rgba8(&mut rgba_uncached);
        fresh.blend_picture_guide(session.world_mut(), &mut rgba_uncached, mx, my);

        assert_eq!(
            fb_cached.pixels, fb_uncached.pixels,
            "fb mismatch at ({mx}, {my}, {scroll})"
        );
        assert_eq!(
            rgba_cached, rgba_uncached,
            "rgba mismatch at ({mx}, {my}, {scroll})"
        );
    }
}

#[test]
fn toolbox_cache_invalidates_on_tray_pickup_and_return_without_bumping_board_rev() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 4, true, true, false);
    let (tbx, font) = synthetic_toolbox_and_font();

    let mut cached_renderer = PuzzleRenderer::new();
    let mut fb_cached = Framebuffer::new();
    let mut fb_uncached = Framebuffer::new();

    // Hover slot 0 at (20, 40) with tray_scroll = 0, then pick up and return at the same coordinates.
    let (mx, my) = (20, 40);
    let initial_board_rev = session.render_versions().board_rev;
    let initial_tray_rev = session.render_versions().tray_rev;

    session.render(&mut cached_renderer, &mut fb_cached, &tbx, &font, mx, my);

    // Pickup from slot 0: tray_order shrinks, board_rev stays unchanged, tray_rev bumps.
    session.handle_click(mx, my);
    assert!(session.held_piece().is_some());
    assert_eq!(session.render_versions().board_rev, initial_board_rev);
    assert_ne!(session.render_versions().tray_rev, initial_tray_rev);
    let pickup_tray_rev = session.render_versions().tray_rev;

    session.render(&mut cached_renderer, &mut fb_cached, &tbx, &font, mx, my);
    let mut fresh = PuzzleRenderer::new();
    session.render(&mut fresh, &mut fb_uncached, &tbx, &font, mx, my);
    assert_eq!(
        fb_cached.pixels, fb_uncached.pixels,
        "stale toolbox after pickup"
    );

    // Return to slot 0 at the exact same hover/scroll: board_rev still unchanged, tray_rev bumps.
    session.handle_click(mx, my);
    assert!(session.held_piece().is_none());
    assert_eq!(session.render_versions().board_rev, initial_board_rev);
    assert_ne!(session.render_versions().tray_rev, pickup_tray_rev);

    session.render(&mut cached_renderer, &mut fb_cached, &tbx, &font, mx, my);
    let mut fresh2 = PuzzleRenderer::new();
    session.render(&mut fresh2, &mut fb_uncached, &tbx, &font, mx, my);
    assert_eq!(
        fb_cached.pixels, fb_uncached.pixels,
        "stale toolbox after return"
    );
}

#[test]
fn hint_targets_first_visible_tray_piece_when_scrolled() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 3, true, false, false);
    session.scroll_tray(2);
    assert_eq!(session.meta().tray_scroll, 2);

    session.trigger_hint();
    let expected_tray_target = session.tray_order()[2];
    assert_eq!(session.hint_piece(), Some((expected_tray_target, 1.5)));

    let held = session.tray_order()[0];
    let held_e = session
        .world_mut()
        .query::<(Entity, &PieceIdentity)>()
        .iter(session.world())
        .find_map(|(e, id)| (id.index == held).then_some(e))
        .unwrap();
    *session.world_mut().get_mut::<PieceLocation>(held_e).unwrap() = PieceLocation::Held {
        is_anchor: true,
        group_offset: (0, 0),
        group_order: 0,
    };
    session.trigger_hint();
    assert_eq!(session.hint_piece(), Some((held, 1.5)));
}

#[test]
fn enter_skip_animation_does_not_spawn_keyboard_pointer() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 2, true, false, false);
    session.step(
        AppInput {
            f2: true,
            ..Default::default()
        },
        0.0,
    );
    assert!(session.active_anim().is_some());
    assert!(session.meta().keyboard_pointer.is_none());

    assert_ne!(session.handle_keyboard_activate(), AppScreen::Browser);
    assert!(session.active_anim().is_none());
    assert!(session.meta().keyboard_pointer.is_none());
}

#[test]
fn move_keyboard_pointer_ignores_non_finite_and_negative_dt() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 2, true, false, false);
    for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
        session.move_keyboard_pointer(0.0, 1.0, dt);
        let (x, y) = session.meta().keyboard_pointer.unwrap();
        assert!(x.is_finite() && (0.0..=319.0).contains(&x));
        assert!(y.is_finite() && (0.0..=199.0).contains(&y));
    }
}

fn pick_piece_from_tray(session: &mut EcsPuzzleHarness, target_idx: usize) {
    let tray = session.tray_order();
    let pos = tray
        .iter()
        .position(|&p| p == target_idx)
        .expect("piece in tray");
    session.meta_mut().tray_scroll = pos;
    session.handle_click(20, 40);
    assert_eq!(session.held_piece(), Some(target_idx));
}

fn cell_center_screen(session: &EcsPuzzleHarness, r: usize, c: usize) -> (i32, i32) {
    let meta = session.meta();
    let col_w = grid_col_widths(meta.grid_dim);
    let row_h = grid_row_heights(meta.grid_dim);
    let cx: i32 = col_w[..c].iter().sum::<i32>() + col_w[c] / 2;
    let cy: i32 = row_h[..r].iter().sum::<i32>() + row_h[r] / 2;
    (meta.playfield_x() + cx, meta.playfield_y() + cy)
}

#[test]
fn straight_edge_grid_allows_non_target_compatible_placement_and_detects_misplaced_completion() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 4, false, false, false);

    // Piece 1 is (0, 1) and piece 2 is (0, 2); in straight-edge mode they have identical edge signatures.
    // Verify piece 1 cannot be placed on corner (0, 0) (which needs a flat left border).
    pick_piece_from_tray(&mut session, 1);
    let (x00, y00) = cell_center_screen(&session, 0, 0);
    session.handle_click(x00, y00);
    assert_eq!(session.held_piece(), Some(1));

    // Place all 16 pieces, swapping pieces 1 and 2 into cells (0, 2) and (0, 1).
    let (x02, y02) = cell_center_screen(&session, 0, 2);
    session.handle_click(x02, y02);
    assert!(session.held_piece().is_none());
    assert_eq!(session.board_cell(0, 2), Some(1));

    pick_piece_from_tray(&mut session, 2);
    let (x01, y01) = cell_center_screen(&session, 0, 1);
    session.handle_click(x01, y01);
    assert_eq!(session.board_cell(0, 1), Some(2));

    for idx in 0..16 {
        if idx == 1 || idx == 2 {
            continue;
        }
        pick_piece_from_tray(&mut session, idx);
        let (x, y) = cell_center_screen(&session, idx / 4, idx % 4);
        session.handle_click(x, y);
        assert!(session.is_placed(idx));
    }

    assert!(!session.meta().solved);
    assert_eq!(
        session.meta().status_message(),
        Some("Not finished yet. Push F1 for a hint or F2 to finish.")
    );

    // F1 highlights the first misplaced piece (piece 2 in cell 1).
    session.trigger_hint();
    assert_eq!(session.hint_piece().map(|(p, _)| p), Some(2));

    // F2 swaps misplaced pieces into place and completes the puzzle.
    session.step(
        AppInput {
            f2: true,
            ..Default::default()
        },
        0.0,
    );
    assert!(session.meta().solved);
    assert_eq!(session.board_cell(0, 1), Some(1));
    assert_eq!(session.board_cell(0, 2), Some(2));
}

#[test]
fn placed_piece_pickup_and_right_click_connected_group_move() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 4, false, false, false);

    // Place pieces (0, 1)=1 and (1, 1)=5 into their home cells.
    pick_piece_from_tray(&mut session, 1);
    let (x01, y01) = cell_center_screen(&session, 0, 1);
    session.handle_click(x01, y01);
    assert!(session.is_placed(1));

    // Left-click placed piece 1 to pick it back up.
    session.handle_click(x01, y01);
    assert_eq!(session.held_piece(), Some(1));
    assert!(!session.is_placed(1));
    assert_eq!(session.board_cell(0, 1), None);

    // Place piece 1 and piece 5 back on board to form a 2-piece vertical cluster.
    session.handle_click(x01, y01);
    pick_piece_from_tray(&mut session, 5);
    let (x11, y11) = cell_center_screen(&session, 1, 1);
    session.handle_click(x11, y11);
    assert!(session.is_placed(1) && session.is_placed(5));

    // Right-click piece 1 to pick up the 2-piece connected group.
    session.handle_right_click(x01, y01);
    assert_eq!(session.held_piece(), Some(1));
    assert_eq!(session.held_group().len(), 2);
    assert!(!session.is_placed(1) && !session.is_placed(5));

    // Move the entire connected group one column right to (0, 2) and (1, 2).
    let (x02, y02) = cell_center_screen(&session, 0, 2);
    session.handle_click(x02, y02);
    assert!(session.held_piece().is_none());
    assert!(session.held_group().is_empty());
    assert_eq!(session.board_cell(0, 2), Some(1));
    assert_eq!(session.board_cell(1, 2), Some(5));
}

#[test]
fn completion_time_message_formatted_when_timer_enabled() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 2, true, false, true);
    session.update(3661.5);
    session.step(
        AppInput {
            f2: true,
            ..Default::default()
        },
        0.0,
    );
    assert!(session.meta().solved);
    assert_eq!(session.meta().status_message(), Some("Your time was 1:01:01"));
}

#[test]
fn replace_scene_asset_preserves_board_cells_and_held_group() {
    let vga_asset = synthetic_puzzle_asset(1, false);
    let ega_asset = synthetic_puzzle_asset(1, true);
    let mut session = EcsPuzzleHarness::new_no_audio(1, vga_asset, 4, false, false, false);

    // Place piece 13 (3, 1) at compatible cell (3, 2).
    pick_piece_from_tray(&mut session, 13);
    let (x32, y32) = cell_center_screen(&session, 3, 2);
    session.handle_click(x32, y32);
    assert_eq!(session.board_cell(3, 2), Some(13));

    // Place pieces 1 (0, 1) and 5 (1, 1) and pick them up as a 2-piece held group.
    pick_piece_from_tray(&mut session, 1);
    let (x01, y01) = cell_center_screen(&session, 0, 1);
    session.handle_click(x01, y01);
    pick_piece_from_tray(&mut session, 5);
    let (x11, y11) = cell_center_screen(&session, 1, 1);
    session.handle_click(x11, y11);
    session.handle_right_click(x01, y01);
    assert_eq!(session.held_piece(), Some(1));
    assert_eq!(session.held_group().len(), 2);

    // Switch video mode mid-puzzle via replace_scene_asset.
    session.replace_scene_asset(ega_asset);
    assert_eq!(session.board_cell(3, 2), Some(13));
    assert!(session.is_placed(13));
    assert_eq!(session.held_piece(), Some(1));
    assert_eq!(session.held_group().len(), 2);

    // Place the preserved held group onto (0, 2)/(1, 2), then pick up piece 13 from (3, 2).
    let (x02, y02) = cell_center_screen(&session, 0, 2);
    session.handle_click(x02, y02);
    assert!(session.held_piece().is_none());
    assert_eq!(session.board_cell(0, 2), Some(1));
    assert_eq!(session.board_cell(1, 2), Some(5));

    session.handle_click(x32, y32);
    assert_eq!(session.held_piece(), Some(13));
    assert_eq!(session.board_cell(3, 2), None);
}

#[test]
fn blend_picture_guide_protects_non_anchor_held_group_pixels_matching_board_bg() {
    let mut palette = [[0u8; 3]; 256];
    palette[1] = [10, 20, 220];
    palette[7] = [204, 204, 187];
    let asset = PuzzleAsset {
        id: 1,
        palette,
        records: vec![SceneRecord::Frame {
            bg_frame_idx: 0,
            bg_color: 1,
            commands: vec![],
        }],
        sequences: Vec::new(),
        hotspots: Vec::new(),
        full_anim_seq_idx: 0,
        sounds: Vec::new(),
        sprites: Vec::new(),
    };
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 4, false, false, false);
    let mut renderer = PuzzleRenderer::new();

    // Pick up pieces 1 (anchor) and 5 (non-anchor) as a 2-piece connected group.
    pick_piece_from_tray(&mut session, 1);
    let (x01, y01) = cell_center_screen(&session, 0, 1);
    session.handle_click(x01, y01);
    pick_piece_from_tray(&mut session, 5);
    let (x11, y11) = cell_center_screen(&session, 1, 1);
    session.handle_click(x11, y11);
    session.handle_right_click(x01, y01);
    assert_eq!(session.held_group().len(), 2);

    let bg_rgba = [204u8, 204, 187, 255];
    let mut rgba = bg_rgba.repeat(320 * 200);
    let pointer_x = x01;
    let pointer_y = y01;
    renderer.blend_picture_guide(session.world_mut(), &mut rgba, pointer_x, pointer_y);

    // Center of non-anchor piece 5 (which had board_bg color in rgba) must remain protected.
    let offsets = {
        let meta = session.meta().clone();
        let mut items: Vec<(usize, bool, (i32, i32), usize)> = session
            .world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(session.world())
            .filter_map(|(id, loc)| match *loc {
                PieceLocation::Held {
                    is_anchor,
                    group_offset,
                    group_order,
                } => Some((id.index, is_anchor, group_offset, group_order)),
                _ => None,
            })
            .collect();
        items.sort_by_key(|&(_, _, _, go)| go);
        crate::ecs::puzzle_ops::held_group_offsets_at(
            &meta,
            pointer_x,
            pointer_y,
            items.iter().map(|&(idx, anc, off, _)| (idx, anc, off)),
        )
    };
    let (_, cell_dx, cell_dy) = offsets[1];
    let anchor = session.piece_geom(1);
    let piece5 = session.piece_geom(5);
    let sx = (pointer_x - anchor.base_w / 2 + cell_dx + piece5.target_x - piece5.base_x
        + piece5.base_w / 2) as usize;
    let sy = (pointer_y - anchor.base_h / 2 + cell_dy + piece5.target_y - piece5.base_y
        + piece5.base_h / 2) as usize;
    let off = (sy * 320 + sx) * 4;
    assert_eq!(&rgba[off..off + 4], &bg_rgba);

    // Uncovered empty cell (3, 3) should have been blended with the blue guide picture.
    let (x33, y33) = cell_center_screen(&session, 3, 3);
    let empty_off = (y33 as usize * 320 + x33 as usize) * 4;
    assert_ne!(&rgba[empty_off..empty_off + 4], &bg_rgba);
}

#[test]
fn held_group_preview_offsets_match_destination_cell_on_uneven_7x7_grid() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset, 7, false, false, false);

    // On a 7x7 grid, col_w = [39, 38, 39, 38, 39, 38, 39]:
    // c1->c2 spans 38 px, whereas c2->c3 spans 39 px.
    let idx_11 = 7 + 1;
    let idx_12 = 7 + 2;
    pick_piece_from_tray(&mut session, idx_11);
    let (x11, y11) = cell_center_screen(&session, 1, 1);
    session.handle_click(x11, y11);
    pick_piece_from_tray(&mut session, idx_12);
    let (x12, y12) = cell_center_screen(&session, 1, 2);
    session.handle_click(x12, y12);

    session.handle_right_click(x11, y11);
    assert_eq!(session.held_group().len(), 2);

    // Hovering over (1, 2) must preview piece idx_12 at +39 px (c2->c3), not +38 px (c1->c2).
    let offsets = {
        let meta = session.meta().clone();
        let mut items: Vec<(usize, bool, (i32, i32), usize)> = session
            .world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(session.world())
            .filter_map(|(id, loc)| match *loc {
                PieceLocation::Held {
                    is_anchor,
                    group_offset,
                    group_order,
                } => Some((id.index, is_anchor, group_offset, group_order)),
                _ => None,
            })
            .collect();
        items.sort_by_key(|&(_, _, _, go)| go);
        crate::ecs::puzzle_ops::held_group_offsets_at(
            &meta,
            x12,
            y12,
            items.iter().map(|&(idx, anc, off, _)| (idx, anc, off)),
        )
    };
    assert_eq!(offsets, vec![(idx_11, 0, 0), (idx_12, 39, 0)]);

    // Placing at (1, 2) snaps idx_11 to (1, 2) and idx_12 to (1, 3), 39 px apart.
    session.handle_click(x12, y12);
    let (ox1, _) = {
        let g = session.piece_geom(idx_11);
        let PieceLocation::Board { row, col } = session
            .world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(session.world())
            .find_map(|(id, l)| (id.index == idx_11).then_some(*l))
            .unwrap()
        else {
            panic!("expected placed");
        };
        crate::ecs::puzzle_ops::piece_placed_origin(session.meta().grid_dim, &g, row, col)
    };
    let (ox2, _) = {
        let g = session.piece_geom(idx_12);
        let PieceLocation::Board { row, col } = session
            .world_mut()
            .query::<(&PieceIdentity, &PieceLocation)>()
            .iter(session.world())
            .find_map(|(id, l)| (id.index == idx_12).then_some(*l))
            .unwrap()
        else {
            panic!("expected placed");
        };
        crate::ecs::puzzle_ops::piece_placed_origin(session.meta().grid_dim, &g, row, col)
    };
    assert_eq!(ox2 - ox1, 39);
}

#[test]
fn non_target_placement_and_center_alignment_across_all_grids() {
    for grid_dim in 2usize..=8 {
        let asset = synthetic_puzzle_asset(1, false);
        let mut session = EcsPuzzleHarness::new_no_audio(1, asset, grid_dim, false, false, false);
        if grid_dim >= 4 {
            // Place interior piece (1, 1) into a different interior cell (2, grid_dim - 2).
            let src_idx = grid_dim + 1;
            pick_piece_from_tray(&mut session, src_idx);
            let dst_r = 2;
            let dst_c = grid_dim - 2;
            assert_ne!((dst_r, dst_c), (1, 1));
            let (dx, dy) = cell_center_screen(&session, dst_r, dst_c);
            session.handle_click(dx, dy);
            assert_eq!(session.board_cell(dst_r, dst_c), Some(src_idx));
            assert!(session.held_piece().is_none());

            // Verify `piece_placed_origin` shifts by exact destination col_x/row_y, and
            // destination half-width matches source half-width (`FUN_1e59_244e` parity).
            let src_geom = session.piece_geom(src_idx);
            let PieceLocation::Board { row, col } = session
                .world_mut()
                .query::<(&PieceIdentity, &PieceLocation)>()
                .iter(session.world())
                .find_map(|(id, l)| (id.index == src_idx).then_some(*l))
                .unwrap()
            else {
                panic!("expected placed");
            };
            let (ox, oy) = crate::ecs::puzzle_ops::piece_placed_origin(
                session.meta().grid_dim,
                &src_geom,
                row,
                col,
            );
            let col_w = grid_col_widths(grid_dim);
            let row_h = grid_row_heights(grid_dim);
            let col_x: i32 = col_w[..dst_c].iter().sum();
            let row_y: i32 = row_h[..dst_r].iter().sum();
            assert_eq!(
                ox - col_x,
                src_geom.target_x - src_geom.base_x,
                "grid {grid_dim}x{grid_dim} x origin"
            );
            assert_eq!(
                oy - row_y,
                src_geom.target_y - src_geom.base_y,
                "grid {grid_dim}x{grid_dim} y origin"
            );
            assert!(
                (col_w[dst_c] - src_geom.base_w).abs() <= 1
                    && (col_w[dst_c] - src_geom.base_w) / 2 == 0,
                "grid {grid_dim}x{grid_dim} width parity"
            );
            assert!(
                (row_h[dst_r] - src_geom.base_h).abs() <= 1
                    && (row_h[dst_r] - src_geom.base_h) / 2 == 0,
                "grid {grid_dim}x{grid_dim} height parity"
            );
        } else {
            // 2x2 and 3x3: corner/border/single-center pieces reject incompatible non-target cells.
            pick_piece_from_tray(&mut session, 0);
            let (dx, dy) = cell_center_screen(&session, grid_dim - 1, grid_dim - 1);
            session.handle_click(dx, dy);
            assert_eq!(session.held_piece(), Some(0));
            assert_eq!(session.board_cell(grid_dim - 1, grid_dim - 1), None);
        }
    }
}

#[test]
fn group_move_rejects_edge_oob_occupied_and_resets_on_restart() {
    let asset = synthetic_puzzle_asset(1, false);
    let mut session = EcsPuzzleHarness::new_no_audio(1, asset.clone(), 4, false, false, false);

    // Place horizontal pair (1, 1)=5 and (1, 2)=6, plus obstacle at (2, 3)=11.
    for (pidx, r, c) in [(5usize, 1usize, 1usize), (6, 1, 2), (11, 2, 3)] {
        pick_piece_from_tray(&mut session, pidx);
        let (cx, cy) = cell_center_screen(&session, r, c);
        session.handle_click(cx, cy);
    }

    // Pick up connected group {5, 6} anchored at (1, 1).
    let (x11, y11) = cell_center_screen(&session, 1, 1);
    session.handle_right_click(x11, y11);
    assert_eq!(session.held_group().len(), 2);

    // 1. Out-of-bounds right edge: placing anchor at col 3 pushes member (dc=+1) to col 4 -> rejected.
    let (x13, y13) = cell_center_screen(&session, 1, 3);
    session.handle_click(x13, y13);
    assert_eq!(session.held_piece(), Some(5));
    assert_eq!(session.held_group().len(), 2);

    // 2. Occupied cell: placing anchor at (2, 2) collides member (dc=+1) with obstacle at (2, 3) -> rejected.
    let (x22, y22) = cell_center_screen(&session, 2, 2);
    session.handle_click(x22, y22);
    assert_eq!(session.held_piece(), Some(5));
    assert_eq!(session.held_group().len(), 2);

    // 3. Interrupted interaction: returning to tray drops both group pieces cleanly.
    session.handle_click(20, 40);
    assert!(session.held_piece().is_none());
    assert!(session.held_group().is_empty());
    assert!(!session.is_placed(5) && !session.is_placed(6));

    // 4. Restarting puzzle clears all board cells and held state.
    let mut restarted = EcsPuzzleHarness::new_no_audio(1, asset, 4, false, false, false);
    assert_eq!(restarted.placed_count(), 0);
    assert!(restarted.held_piece().is_none());
    assert!(restarted.held_group().is_empty());
}

#[test]
fn bezier_edge_geometry_is_deterministic_and_complementary_across_all_grids() {
    use crate::ecs::puzzle_ops::{cut_pieces_data, render_full_picture_into};

    for is_vert in [false, true] {
        for span in [20, 32, 47] {
            for variant in 0..8 {
                let a = edge_curve(is_vert, span, variant);
                let b = edge_curve(is_vert, span, variant);
                assert_eq!(a, b, "edge_curve must be deterministic");
                assert!(!a.1.is_empty());
            }
            let v0 = edge_curve(is_vert, span, 0);
            let v1 = edge_curve(is_vert, span, 1);
            assert!(v0.0 > 0 && v1.0 > 0);
            assert_ne!(v0.1, v1.1);
        }
    }

    let asset = synthetic_puzzle_asset(1, false);
    let mut full_fb = Framebuffer::new();
    render_full_picture_into(&asset, &mut full_fb);
    for dim in 2..=8 {
        for curved in [false, true] {
            let (pieces, polylines_v, polylines_h) =
                cut_pieces_data(1, &asset, &full_fb, dim, curved);
            assert_eq!(pieces.len(), dim * dim);
            assert_eq!(polylines_v.len(), dim - 1);
            assert_eq!(polylines_h.len(), dim - 1);
        }
    }
}
