pub mod app;
pub mod assets;
pub mod audio;
pub mod decompress;
pub mod graphics;
pub mod puzzle;
pub mod puzzle_render;
pub mod setup;

use std::env;
use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use crate::app::{AppInput, AppRenderer, AppScreen, GameApp, game_viewport};
use crate::graphics::{SCREEN_H, SCREEN_W};
use crate::puzzle::PuzzleSession;

fn find_game_dir() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("MJP.NFT").exists() {
        return cwd;
    }
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    if exe_dir.join("MJP.NFT").exists() {
        return exe_dir;
    }
    cwd
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Mickey's Jigsaw Puzzles (1991 DOS Recreation)".to_string(),
        window_width: 960,
        window_height: 720,
        window_resizable: true,
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn render_desktop_hud(
    session: &PuzzleSession,
    win_w: f32,
    win_h: f32,
    reference: &Texture2D,
    cached_ref_key: &mut Option<(u64, u64)>,
    rgba_buf: &mut [u8],
    reference_button: Rect,
    reference_hovered: bool,
    show_reference: bool,
) {
    let ink = Color::from_hex(0xf5edd8);
    let muted = Color::from_hex(0xb2c4ca);
    let accent = Color::from_hex(0xedbe5e);
    draw_rectangle(0.0, 0.0, win_w, 80.0, Color::from_hex(0x182831));
    let ref_key = (session.session_id(), session.asset_rev());
    if *cached_ref_key != Some(ref_key) {
        session.full_picture_fb.to_rgba8(rgba_buf);
        reference.update_from_bytes(SCREEN_W as u32, SCREEN_H as u32, rgba_buf);
        *cached_ref_key = Some(ref_key);
    }
    draw_texture_ex(
        reference,
        16.0,
        10.0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(83.0, 60.0)),
            source: Some(Rect::new(2.0, 2.0, 270.0, 196.0)),
            ..Default::default()
        },
    );
    let compact = win_w < 720.0;
    draw_text(
        if compact {
            "Mickey's Puzzles"
        } else {
            "Mickey's Jigsaw Puzzles"
        },
        112.0,
        31.0,
        if compact { 20.0 } else { 25.0 },
        ink,
    );
    let placed = session.placed_count();
    draw_text(
        &if compact {
            format!(
                "{:02}  /  {} of {}",
                session.puzzle_num,
                placed,
                session.pieces.len()
            )
        } else {
            format!(
                "Picture {:02}   /   {} of {} pieces placed",
                session.puzzle_num,
                placed,
                session.pieces.len()
            )
        },
        112.0,
        57.0,
        18.0,
        muted,
    );
    if session.timer_enabled {
        let time = session.formatted_time();
        let width = measure_text(&time, None, 24, 1.0).width;
        draw_text(&time, win_w - width - 20.0, 29.0, 24.0, ink);
    }
    draw_rectangle(
        reference_button.x,
        reference_button.y,
        reference_button.w,
        reference_button.h,
        if show_reference {
            accent
        } else if reference_hovered && !session.solved {
            Color::from_hex(0x496675)
        } else {
            Color::from_hex(0x364c57)
        },
    );
    draw_text(
        if session.solved {
            "Picture complete!"
        } else if show_reference {
            "Release to keep playing"
        } else {
            "Hold: picture  [Tab]"
        },
        reference_button.x + 12.0,
        reference_button.y + 21.0,
        18.0,
        if show_reference {
            Color::from_hex(0x182831)
        } else {
            ink
        },
    );
    draw_rectangle(0.0, win_h - 36.0, win_w, 36.0, Color::from_hex(0x182831));
    let help = if compact {
        "Tab: picture   H: guide   F1: hint   Esc: menu"
    } else if show_reference {
        "Picture reference  /  progress and timer paused"
    } else if session.solved {
        "Click the picture to play   /   Esc: choose another puzzle"
    } else if session.held_piece.is_some() {
        "Place on the board   /   Click the tray to return the piece"
    } else {
        "Pick and place   /   H: picture guide   G: edges   F1: hint   Esc: menu"
    };
    draw_text(help, 16.0, win_h - 13.0, 18.0, muted);
}

fn poll_app_input(dt: f32, mx: i32, my: i32, inside: bool) -> AppInput {
    let (_, wheel_y) = mouse_wheel();
    let wheel_delta = if wheel_y > 0.0 {
        1
    } else if wheel_y < 0.0 {
        -1
    } else {
        0
    };
    AppInput {
        dt,
        pointer_x: mx,
        pointer_y: my,
        left_pressed: inside && is_mouse_button_pressed(MouseButton::Left),
        left_released: is_mouse_button_released(MouseButton::Left),
        right_pressed: inside && is_mouse_button_pressed(MouseButton::Right),
        wheel_delta,
        up: is_key_pressed(KeyCode::Up),
        down: is_key_pressed(KeyCode::Down),
        left: is_key_pressed(KeyCode::Left),
        right: is_key_pressed(KeyCode::Right),
        enter: is_key_pressed(KeyCode::Enter),
        space: is_key_pressed(KeyCode::Space),
        escape: is_key_pressed(KeyCode::Escape),
        tab: is_key_pressed(KeyCode::Tab),
        key_e: is_key_pressed(KeyCode::E),
        key_r: is_key_pressed(KeyCode::R),
        key_s: is_key_pressed(KeyCode::S),
        key_g: is_key_pressed(KeyCode::G),
        key_p: is_key_pressed(KeyCode::P),
        f1: is_key_pressed(KeyCode::F1),
        f2: is_key_pressed(KeyCode::F2),
        f3: is_key_pressed(KeyCode::F3),
        f4: is_key_pressed(KeyCode::F4),
        f5: is_key_pressed(KeyCode::F5),
        move_x: i32::from(is_key_down(KeyCode::D)) - i32::from(is_key_down(KeyCode::A)),
        move_y: i32::from(is_key_down(KeyCode::S)) - i32::from(is_key_down(KeyCode::W)),
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let start_in_setup = args
        .iter()
        .any(|a| a.eq_ignore_ascii_case("/S") || a.eq_ignore_ascii_case("--setup"));

    let game_dir = find_game_dir();
    let mut app = match GameApp::new(game_dir, start_in_setup) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Failed to initialize Mickey's Jigsaw Puzzles: {}", e);
            return;
        }
    };

    let mut renderer = AppRenderer::new();
    let texture = Texture2D::from_rgba8(SCREEN_W as u16, SCREEN_H as u16, &renderer.rgba_buf);
    texture.set_filter(FilterMode::Nearest);
    let reference = Texture2D::from_rgba8(SCREEN_W as u16, SCREEN_H as u16, &renderer.rgba_buf);
    reference.set_filter(FilterMode::Nearest);
    let mut cached_ref_key: Option<(u64, u64)> = None;
    let mut picture_guide = true;
    let mut mouse_reference = false;

    loop {
        let dt = get_frame_time().clamp(0.0, 0.25);

        let win_w = screen_width();
        let win_h = screen_height();
        let playing = app.screen == AppScreen::Playing;
        if playing && is_key_pressed(KeyCode::H) {
            picture_guide = !picture_guide;
        }
        let viewport = game_viewport(win_w, win_h, playing);
        let scale = viewport.w / SCREEN_W as f32;

        let (raw_mx, raw_my) = mouse_position();
        let mx = ((raw_mx - viewport.x) / scale).floor() as i32;
        let my = ((raw_my - viewport.y) / scale).floor() as i32;
        let inside = (0..SCREEN_W).contains(&mx) && (0..SCREEN_H).contains(&my);

        let reference_button = Rect::new(win_w - 210.0, 42.0, 194.0, 30.0);
        let reference_hovered =
            win_w >= 480.0 && win_h >= 360.0 && reference_button.contains(vec2(raw_mx, raw_my));
        let reference_available = playing && app.session.as_ref().is_some_and(|s| !s.solved);
        mouse_reference = reference_available
            && is_mouse_button_down(MouseButton::Left)
            && (mouse_reference
                || (reference_hovered && is_mouse_button_pressed(MouseButton::Left)));
        let show_reference = reference_available && (is_key_down(KeyCode::Tab) || mouse_reference);
        if show_reference {
            if let Some(session) = &mut app.session {
                session.cancel_pointer_drag();
            }
        } else {
            let input = poll_app_input(dt, mx, my, inside);
            app.handle_input(&input);
        }

        if renderer.render_if_needed(&app, mx, my, show_reference, picture_guide) {
            texture.update_from_bytes(SCREEN_W as u32, SCREEN_H as u32, &renderer.rgba_buf);
        }

        let viewport = game_viewport(win_w, win_h, app.screen == AppScreen::Playing);
        clear_background(Color::from_hex(0x263640));
        draw_texture_ex(
            &texture,
            viewport.x,
            viewport.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(viewport.w, viewport.h)),
                ..Default::default()
            },
        );

        if app.screen == AppScreen::Playing && win_w >= 480.0 && win_h >= 360.0 {
            if let Some(session) = &app.session {
                render_desktop_hud(
                    session,
                    win_w,
                    win_h,
                    &reference,
                    &mut cached_ref_key,
                    &mut renderer.rgba_buf,
                    reference_button,
                    reference_hovered,
                    show_reference,
                );
            }
        } else if app.screen == AppScreen::Playing
            && let Some(session) = app.session.as_ref().filter(|s| s.timer_enabled)
        {
            let time = session.formatted_time();
            draw_text(&time, win_w - 100.0, 18.0, 18.0, WHITE);
        }

        next_frame().await;
    }
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
