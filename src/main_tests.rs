use super::*;
use crate::app::{AppInput, AppRenderer, AppScreen, BROWSER_BUTTONS, GameApp, game_viewport};
use crate::assets::{
    FontAsset, Preferences, PrintAsset, PuzzleAsset, ToolboxAsset, load_ilbm_container,
};
use crate::audio::{AudioEngine, CommonSound, synthesize_iss_song, synthesize_nnv_phrase};
use crate::graphics::{Framebuffer, SCREEN_W};
use crate::puzzle::{PuzzleAction, PuzzleSession};
use crate::puzzle_render::PuzzleRenderer;
use crate::setup::SetupMenu;

#[test]
fn play_viewport_preserves_scene_and_input_coordinates() {
    for (width, height) in [
        (960.0, 720.0),
        (1700.0, 1380.0),
        (480.0, 360.0),
        (320.0, 200.0),
    ] {
        let viewport = game_viewport(width, height, true);
        assert!(viewport.x >= 0.0 && viewport.y >= 0.0);
        assert!(viewport.x + viewport.w <= width + 0.01);
        assert!(viewport.y + viewport.h <= height + 0.01);
        assert!((viewport.w / viewport.h - 1.6).abs() < 0.001);
        let scale = viewport.w / SCREEN_W as f32;
        for (x, y) in [(20.0, 40.0), (115.0, 51.0), (250.0, 149.0)] {
            let px = viewport.x + (x + 0.5) * scale;
            let py = viewport.y + (y + 0.5) * scale;
            assert_eq!(((px - viewport.x) / scale).floor() as i32, x as i32);
            assert_eq!(((py - viewport.y) / scale).floor() as i32, y as i32);
        }
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn picture_guide_preserves_placed_and_held_artwork() {
    let dir = test_game_dir();
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        let asset = PuzzleAsset::load(&dir, 1, ega).unwrap();
        let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(ega)).unwrap();
        let mut session = PuzzleSession::new(1, asset, 3, true, false, false);
        let mut renderer = PuzzleRenderer::new();
        let original = session.full_picture_fb.pixels;
        session.set_piece_placed_for_test(0, true);
        session.held_piece = Some(1);
        let mut fb = Framebuffer::new();
        renderer.render(&session, &mut fb, &toolbox, &font, 160, 100);
        let mut before = vec![0; 320 * 200 * 4];
        fb.to_rgba8(&mut before);
        let mut after = before.clone();
        renderer.blend_picture_guide(&session, &mut after, 160, 100);
        assert_ne!(before, after, "ega={ega}");
        for (index, held) in [(0, false), (1, true)] {
            let piece = &session.pieces[index];
            let (x, y) = if held {
                (
                    160 - piece.base_w / 2 + piece.target_x - piece.base_x,
                    100 - piece.base_h / 2 + piece.target_y - piece.base_y,
                )
            } else {
                (
                    session.playfield_x() + piece.target_x,
                    session.playfield_y() + piece.target_y,
                )
            };
            for dy in 0..i32::from(piece.sprite.sub_h) {
                for dx in 0..i32::from(piece.sprite.width) {
                    let source = (dy + i32::from(piece.sprite.sub_y)) as usize
                        * usize::from(piece.sprite.stride)
                        + (dx + i32::from(piece.sprite.sub_x)) as usize;
                    if !(0..320).contains(&(x + dx))
                        || !(0..200).contains(&(y + dy))
                        || !piece.sprite.is_opaque(source)
                    {
                        continue;
                    }
                    let offset = (((y + dy) * 320 + x + dx) * 4) as usize;
                    assert_eq!(
                        &before[offset..offset + 4],
                        &after[offset..offset + 4],
                        "ega={ega}, piece={index}"
                    );
                }
            }
        }
        assert_eq!(session.full_picture_fb.pixels, original);
        session.solved = true;
        let mut solved = before.clone();
        renderer.blend_picture_guide(&session, &mut solved, 160, 100);
        assert_eq!(solved, before);
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn completed_picture_is_unchanged_by_timer_setting() {
    let dir = test_game_dir();
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        let asset = PuzzleAsset::load(&dir, 1, ega).unwrap();
        let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(ega)).unwrap();
        let mut session = PuzzleSession::new(1, asset, 2, true, false, false);
        let mut renderer = PuzzleRenderer::new();
        session.solved = true;
        session.elapsed_secs = 3661.0;
        let mut without_timer = Framebuffer::new();
        renderer.render(&session, &mut without_timer, &toolbox, &font, 160, 100);
        session.timer_enabled = true;
        let mut with_timer = Framebuffer::new();
        renderer.render(&session, &mut with_timer, &toolbox, &font, 160, 100);
        let changed = with_timer
            .pixels
            .iter()
            .zip(&without_timer.pixels)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(changed, 0, "ega={ega}: timer altered completed artwork");
    }
}

fn test_game_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn test_browser() -> GameApp {
    let game_dir = test_game_dir();
    let prefs = Preferences {
        music_on: 0,
        sfx_on: 0,
        ..Preferences::default()
    };
    GameApp::from_parts(
        game_dir.clone(),
        FontAsset::load(&game_dir.join("MJP.NFT")).unwrap(),
        ToolboxAsset::load(&game_dir.join("MJP.TBX"), 0).unwrap(),
        ToolboxAsset::load(&game_dir.join("MJP.TBX"), 1).unwrap(),
        PuzzleAsset::load(&game_dir, 1, false).unwrap(),
        prefs,
        false,
    )
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn preset_progress_survives_resume_and_video_switching() {
    let mut app = test_browser();
    app.selected_grid_dim = 1;
    app.start_selected_puzzle();
    let session = app.session.as_mut().unwrap();
    let first = session.tray_order.remove(0);
    session.pieces[first].placed = true;
    let held = session.tray_order.remove(0);
    session.held_piece = Some(held);
    let order = session.tray_order.clone();
    app.screen = AppScreen::Browser;
    app.curved_edges = !app.curved_edges;
    assert!(app.can_resume_selected());
    app.start_selected_puzzle();
    let ega = PuzzleAsset::load(&app.game_dir, 1, true).unwrap();
    app.session.as_mut().unwrap().replace_scene_asset(ega);
    let session = app.session.as_ref().unwrap();
    assert_eq!(session.grid_dim, 1);
    assert!(session.pieces[first].placed);
    assert_eq!(session.held_piece, Some(held));
    assert_eq!(session.tray_order, order);
    assert!(session.asset.uses_ega());
    assert!((11..=16).contains(&session.pieces.len()));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn common_captions_render_and_respect_text_preference() {
    let mut app = test_browser();
    let mut renderer = AppRenderer::new();
    for ega in [false, true] {
        app.preview_asset = PuzzleAsset::load(&app.game_dir, 1, ega).unwrap();
        app.prefs.text_msg_on = 0;
        renderer.render(&app, 160, 100);
        let baseline = renderer.fb.clone();
        app.audio.play_common(CommonSound::ChooseSize);
        renderer.render(&app, 160, 100);
        let hidden = renderer.fb.clone();
        assert_eq!(baseline.pixels, hidden.pixels);
        app.prefs.text_msg_on = 1;
        renderer.render(&app, 160, 100);
        let shown = renderer.fb.clone();
        assert_ne!(baseline.pixels, shown.pixels);
        assert_eq!(&baseline.pixels[164 * 320..], &shown.pixels[164 * 320..]);
        app.audio.update(60.0);
        renderer.render(&app, 160, 100);
        assert_eq!(baseline.pixels, renderer.fb.pixels);
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn preset_pickup_captions_have_visible_glyphs() {
    let mut app = test_browser();
    let mut renderer = AppRenderer::new();
    app.prefs.text_msg_on = 1;
    app.selected_grid_dim = 1;
    for ega in [false, true] {
        app.prefs.video_mode = if ega { 2 } else { 0 };
        for num in 1..=15 {
            app.selected_puzzle = num;
            app.restart_selected_puzzle();
            let session = app.session.as_mut().unwrap();
            session.handle_click(20, 40, &mut app.audio);
            let held = session.held_piece.unwrap();
            let piece = &session.pieces[held];
            let (x, y) = (
                session.playfield_x() + piece.base_x + piece.base_w / 2,
                session.playfield_y() + piece.base_y + piece.base_h / 2,
            );
            session.handle_click(x, y, &mut app.audio);
            renderer.render(&app, 0, 0);
            let fb = &renderer.fb;
            let black = fb.color_index([0; 3]);
            let count = (150..160)
                .flat_map(|y| (5..315).map(move |x| (x, y)))
                .filter(|&(x, y)| fb.get_pixel(x, y) == black)
                .count();
            assert!(count > 100, "P{num:02} ega={ega} glyph pixels={count}");
        }
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn setup_video_change_preserves_active_progress_and_rolls_back_failure() {
    let mut app = test_browser();
    app.start_selected_puzzle();
    let session = app.session.as_mut().unwrap();
    session.handle_click(20, 40, &mut app.audio);
    let placed = session.held_piece.unwrap();
    let piece = &session.pieces[placed];
    let target = (
        session.playfield_x() + piece.base_x + piece.base_w / 2,
        session.playfield_y() + piece.base_y + piece.base_h / 2,
    );
    session.handle_click(target.0, target.1, &mut app.audio);
    session.handle_click(20, 40, &mut app.audio);
    session.elapsed_secs = 123.0;
    session.tray_scroll = 1;
    session.toolbox_open = false;
    let held = session.held_piece;
    session.hint_piece = held.map(|index| (index, 1.0));
    let tray = session.tray_order.clone();
    for mode in [2, 1] {
        app.setup_menu.prefs.video_mode = mode;
        app.sync_prefs_from_setup();
        let session = app.session.as_ref().unwrap();
        assert_eq!(session.asset.sprites[0].bpp, if mode == 2 { 4 } else { 8 });
        assert!(session.pieces[placed].placed);
        assert_eq!(session.held_piece, held);
        assert_eq!(session.tray_order, tray);
        assert_eq!(session.tray_scroll, 1);
        assert!(!session.toolbox_open);
        assert_eq!(session.hint_piece, held.map(|index| (index, 1.0)));
        assert_eq!(session.elapsed_secs, 123.0);
        assert!(app.can_resume_selected());
    }
    let palette = app.session.as_ref().unwrap().asset.palette;
    app.game_dir = PathBuf::from("/dev/null");
    app.setup_menu.prefs.video_mode = 2;
    app.sync_prefs_from_setup();
    assert!(app.error.is_some());
    assert_eq!(app.prefs.video_mode, 1);
    assert_eq!(app.setup_menu.prefs.video_mode, 1);
    let session = app.session.as_ref().unwrap();
    assert_eq!(session.asset.palette, palette);
    assert_eq!(session.held_piece, held);
    assert!(session.pieces[placed].placed);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn setup_video_change_keeps_solved_state_and_supports_ega_fallback() {
    let mut app = test_browser();
    app.selected_puzzle = 6;
    app.start_selected_puzzle();
    app.session.as_mut().unwrap().auto_solve(&mut app.audio);
    assert!(app.session.as_ref().unwrap().active_anim.is_some());
    app.setup_menu.prefs.video_mode = 2;
    app.sync_prefs_from_setup();
    let session = app.session.as_ref().unwrap();
    assert!(session.solved);
    assert!(session.pieces.iter().all(|piece| piece.placed));
    assert!(session.active_anim.is_none());
    assert_eq!(session.asset.sprites[0].bpp, 8);
    assert!(app.session_use_ega);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn video_switch_is_atomic_when_preview_and_paused_puzzle_differ() {
    let mut app = test_browser();
    app.start_selected_puzzle();
    app.selected_puzzle = 2;
    app.reload_preview();
    let old_preview = app.preview_asset.palette;
    let old_active = app.session.as_ref().unwrap().asset.palette;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mickey-video-{unique}"));
    std::fs::create_dir(&dir).unwrap();
    std::fs::copy(app.game_dir.join("P02.PZ4"), dir.join("P02.PZ4")).unwrap();
    app.game_dir = dir.clone();
    app.setup_menu.prefs.video_mode = 2;
    app.sync_prefs_from_setup();
    app.game_dir = test_game_dir();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(app.error.is_some());
    assert_eq!(app.prefs.video_mode, 1);
    assert_eq!(app.preview_asset.palette, old_preview);
    assert_eq!(app.session.as_ref().unwrap().asset.palette, old_active);
    assert!(!app.session_use_ega);
    app.setup_menu.prefs.video_mode = 2;
    app.sync_prefs_from_setup();
    assert!(app.error.is_none());
    assert_eq!(app.preview_asset.id, 2);
    assert_eq!(app.preview_asset.sprites[0].bpp, 4);
    assert_eq!(app.session.as_ref().unwrap().puzzle_num, 1);
    assert_eq!(app.session.as_ref().unwrap().asset.sprites[0].bpp, 4);
    app.selected_puzzle = 1;
    app.reload_preview();
    assert!(app.can_resume_selected());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_resume_preserves_board_tray_held_piece_and_time() {
    let mut app = test_browser();
    app.start_selected_puzzle();
    let session = app.session.as_mut().unwrap();
    session.handle_click(20, 40, &mut app.audio);
    let placed = session.held_piece.unwrap();
    let piece = &session.pieces[placed];
    let target = (
        session.playfield_x() + piece.base_x + piece.base_w / 2,
        session.playfield_y() + piece.base_y + piece.base_h / 2,
    );
    session.handle_click(target.0, target.1, &mut app.audio);
    session.handle_click(20, 40, &mut app.audio);
    session.elapsed_secs = 123.0;
    let held = session.held_piece;
    let tray = session.tray_order.clone();
    app.screen = AppScreen::Browser;
    assert!(app.can_resume_selected());
    app.start_selected_puzzle();
    let session = app.session.as_ref().unwrap();
    assert_eq!(app.screen, AppScreen::Playing);
    assert!(session.pieces[placed].placed);
    assert_eq!(session.held_piece, held);
    assert_eq!(session.tray_order, tray);
    assert_eq!(session.elapsed_secs, 123.0);
    app.game_dir = PathBuf::from("/dev/null");
    app.restart_selected_puzzle();
    assert!(app.error.is_some());
    assert!(app.session.as_ref().unwrap().pieces[placed].placed);
    assert_eq!(app.session.as_ref().unwrap().held_piece, held);
    app.game_dir = test_game_dir();
    app.restart_selected_puzzle();
    let session = app.session.as_ref().unwrap();
    assert!(session.pieces.iter().all(|piece| !piece.placed));
    assert!(session.held_piece.is_none());
    assert_eq!(session.tray_order.len(), 9);
    assert_eq!(session.elapsed_secs, 0.0);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_only_resumes_matching_unfinished_puzzles() {
    let mut app = test_browser();
    assert!(!app.can_resume_selected());
    app.start_selected_puzzle();
    assert!(app.can_resume_selected());
    app.selected_puzzle = 2;
    assert!(!app.can_resume_selected());
    app.selected_puzzle = 1;
    app.selected_grid_dim = 4;
    assert!(!app.can_resume_selected());
    app.selected_grid_dim = 3;
    app.curved_edges = false;
    assert!(!app.can_resume_selected());
    app.curved_edges = true;
    app.prefs.video_mode = 2;
    assert!(!app.can_resume_selected());
    app.prefs.video_mode = 1;
    app.session.as_mut().unwrap().solved = true;
    assert!(!app.can_resume_selected());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn every_puzzle_can_start_in_debug_builds() {
    for num in 1..=15 {
        let asset = PuzzleAsset::load(&test_game_dir(), num, false).unwrap();
        let session = PuzzleSession::new(num, asset, 3, true, true, true);
        assert_eq!(session.pieces.len(), 9);
    }
}

#[test]
fn masked_sprites_preserve_every_palette_index() {
    let pixels: Vec<u8> = (0..=255).chain(std::iter::once(248)).collect();
    let mut opacity = vec![true; 257];
    opacity[256] = false;
    let sprite = assets::Sprite8061 {
        stride: 257,
        height: 1,
        transparent_color: 248,
        bpp: 8,
        sub_x: 0,
        sub_y: 0,
        width: 257,
        sub_h: 1,
        pixels: pixels.clone(),
        opacity: Some(opacity),
    };
    let mut fb = Framebuffer::new();
    fb.clear(9);
    fb.blit_sprite(&sprite, 0, 0, 0);
    fb.blit_thumbnail(&sprite, 0, 1, 257, 1);
    assert_eq!(&fb.pixels[..256], &pixels[..256]);
    assert_eq!(&fb.pixels[320..576], &pixels[..256]);
    assert_eq!(fb.pixels[256], 9);
    assert_eq!(fb.pixels[576], 9);
    fb.blit_sprite(&sprite, 0, 2, (5 << 8) | 0x20);
    assert!(fb.pixels[640..896].iter().all(|&color| color == 5));
    assert_eq!(fb.pixels[896], 9);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn cut_pieces_cover_the_picture_exactly_once() {
    for ega in [false, true] {
        for num in 1..=15 {
            let asset = PuzzleAsset::load(&test_game_dir(), num, ega).unwrap();
            for dim in 2..=8 {
                for curved in [false, true] {
                    let session = PuzzleSession::new(num, asset.clone(), dim, curved, true, true);
                    let mut coverage = vec![0u8; 270 * 196];
                    for piece in &session.pieces {
                        for y in 0..piece.sprite.sub_h as i32 {
                            for x in 0..piece.sprite.width as i32 {
                                let color = piece.sprite.pixels
                                    [(y * piece.sprite.width as i32 + x) as usize];
                                if piece
                                    .sprite
                                    .is_opaque((y * piece.sprite.width as i32 + x) as usize)
                                {
                                    let px = piece.target_x + x;
                                    let py = piece.target_y + y;
                                    assert!((0..270).contains(&px) && (0..196).contains(&py));
                                    coverage[(py * 270 + px) as usize] += 1;
                                    assert_eq!(
                                        color,
                                        session.full_picture_fb.get_pixel(2 + px, 2 + py),
                                        "P{num:02} ega={ega} dim={dim} curved={curved} x={px} y={py}"
                                    );
                                }
                            }
                        }
                    }
                    assert!(
                        coverage.iter().all(|&n| n == 1),
                        "dim={dim} curved={curved}: missing={} overlapping={}",
                        coverage.iter().filter(|&&n| n == 0).count(),
                        coverage.iter().filter(|&&n| n > 1).count()
                    );
                }
            }
        }
    }
}

#[test]
fn truncated_container_is_rejected() {
    assert!(decompress::split_multi_resource(&[1, 0, 4, 0, 0, 0, 42]).is_err());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn truncated_da_is_rejected() {
    let mut raw = vec![0; 20];
    raw[..4].copy_from_slice(b"DA\0\0");
    raw[16] = 4;
    assert!(decompress::decompress_da(&raw).is_err());
    let mut compressed = std::fs::read(test_game_dir().join("P01.PZ8")).unwrap();
    compressed.truncate(compressed.len() / 2);
    assert!(decompress::decompress_da(&compressed).is_err());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn truncated_ilbm_is_rejected() {
    let raw = std::fs::read(test_game_dir().join("MJP.TTL")).unwrap();
    let decoded = decompress::decompress_da(&raw).unwrap();
    let chunks = decompress::split_multi_resource(&decoded).unwrap();
    let image = chunks[0];
    assert!(decompress::decode_ilbm(&image[..image.len() - 1]).is_err());
}

#[test]
fn truncated_sprite_is_rejected() {
    let mut rec = vec![0; 26];
    rec[..2].copy_from_slice(&0x8061u16.to_le_bytes());
    for offset in [10, 12, 22, 24] {
        rec[offset] = 1;
    }
    rec[16] = 8;
    assert!(assets::Sprite8061::from_record(&rec).is_err());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn test_load_font_and_measure_text() {
    let dir = test_game_dir();
    let font = FontAsset::load(&dir.join("MJP.NFT")).expect("MJP.NFT should load");
    assert_eq!(font.height, 10);
    assert_eq!(font.bitmap.len(), 770);

    let w = font.measure_text("MICKEY'S JIGSAW PUZZLES");
    assert!(w > 80);

    let mut fb = Framebuffer::new();
    fb.draw_text_shadowed(&font, "Hello Mickey!", 10, 10, 0, 15);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn test_load_title_and_credits_ilbm() {
    let dir = test_game_dir();
    let ttl = load_ilbm_container(&dir.join("MJP.TTL")).expect("MJP.TTL should load");
    assert!(!ttl.is_empty());
    assert_eq!(ttl[0].width, 320);
    assert_eq!(ttl[0].height, 200);
    assert_eq!(ttl[0].pixels.len(), 64000);

    let error = load_ilbm_container(&dir.join("MJP.CRD")).unwrap_err();
    assert_eq!(error, "Truncated DA bitstream");
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn test_load_toolbox() {
    let dir = test_game_dir();
    let tbx = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).expect("MJP.TBX should load");
    assert_eq!(tbx.sprites.len(), 65);
    // Border sprites 0x38..=0x3C are used by FUN_1b43_0363
    assert!(tbx.sprites.len() > 0x3C);
    for cursor in &tbx.sprites[61..=64] {
        assert_eq!(
            (
                cursor.sub_x,
                cursor.sub_y,
                cursor.width,
                cursor.height,
                cursor.sub_h
            ),
            (0, 0, 16, 16, 16)
        );
    }
    let ega = ToolboxAsset::load(&dir.join("MJP.TBX"), 1).unwrap();
    assert_eq!(ega.sprites.len(), 62);
    assert!(ega.sprites.iter().all(|sprite| sprite.bpp == 4));
    let cga = ToolboxAsset::load(&dir.join("MJP.TBX"), 2).unwrap();
    assert_eq!(cga.sprites.len(), 62);
    assert!(cga.sprites.iter().all(|sprite| sprite.bpp == 2));
    assert_eq!(cga.palette[3], [0, 170, 0]);
    let decoded = decompress::decompress_da(&std::fs::read(dir.join("MJP.TBX")).unwrap()).unwrap();
    let chunks = decompress::split_multi_resource(&decoded).unwrap();
    let mut checked = 0;
    for row in include_str!("../validation/toolbox-oracle.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        let index: usize = fields[0].parse().unwrap();
        let bytes = chunks[index];
        assert_eq!(bytes.len(), fields[1].parse::<usize>().unwrap());
        let hash = bytes.iter().fold(14695981039346656037u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(1099511628211)
        });
        assert_eq!(hash, u64::from_str_radix(fields[5], 16).unwrap());
        checked += 1;
    }
    assert_eq!(checked, 3);
    assert!(ToolboxAsset::load(&dir.join("MJP.TBX"), 3).is_err());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn vga_palette_preserves_original_rgb_bytes() {
    let dir = test_game_dir();
    for num in 1..=15 {
        let raw = std::fs::read(dir.join(format!("P{num:02}.PZ8"))).unwrap();
        let decoded = decompress::decompress_da(&raw).unwrap();
        let asset = PuzzleAsset::load(&dir, num, false).unwrap();
        let mut pos = 0;
        let mut expected = None;
        while pos + 8 <= decoded.len() {
            let tag = &decoded[pos..pos + 4];
            let size = u32::from_le_bytes(decoded[pos + 4..pos + 8].try_into().unwrap()) as usize;
            if tag == b"PALE" {
                expected = Some(&decoded[pos + 8..pos + 8 + size]);
            }
            if tag == b"MCG0" {
                break;
            }
            pos += 8 + size;
        }
        let expected = expected.unwrap();
        for (i, color) in asset.palette.iter().enumerate() {
            assert_eq!(
                color.as_slice(),
                &expected[i * 3..i * 3 + 3],
                "puzzle {num}, color {i}"
            );
        }
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn ega_palette_has_no_six_byte_header() {
    let dir = test_game_dir();
    for num in 1..=15 {
        let path = dir.join(format!("P{num:02}.PZ4"));
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read(path).unwrap();
        let decoded = decompress::decompress_da(&raw).unwrap();
        assert_eq!(&decoded[..4], b"PALE");
        let parsed = PuzzleAsset::from_decompressed(num, &decoded);
        if num == 6 {
            assert!(parsed.is_err());
            continue;
        }
        let asset = parsed.unwrap();
        for i in 0..16 {
            assert_eq!(
                asset.palette[i].as_slice(),
                &decoded[8 + i * 3..11 + i * 3],
                "puzzle {num}, color {i}"
            );
        }
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn test_load_all_15_puzzles_vga_and_ega_and_prt() {
    let dir = test_game_dir();
    for num in 1..=15 {
        let pz8 = PuzzleAsset::load(&dir, num, false)
            .unwrap_or_else(|e| panic!("P{:02}.PZ8 failed: {}", num, e));
        assert!(
            !pz8.sprites.is_empty(),
            "P{:02}.PZ8 should have sprites",
            num
        );
        assert!(
            pz8.sequences.len() >= 2,
            "P{:02}.PZ8 should have sequences",
            num
        );

        let pz4 = PuzzleAsset::load(&dir, num, true)
            .unwrap_or_else(|e| panic!("P{:02}.PZ4 failed: {}", num, e));
        assert!(!pz4.sprites.is_empty());
        assert_eq!(pz4.id, num);
        let expected_bpp = if dir.join(format!("P{num:02}.PZ4")).exists() && num != 6 {
            4
        } else {
            8
        };
        assert!(pz4.sprites.iter().all(|sprite| sprite.bpp == expected_bpp));

        let prt =
            PrintAsset::load(&dir, num).unwrap_or_else(|e| panic!("P{:02}.PRT failed: {}", num, e));
        assert_eq!(prt.bitmap.len(), 8000);
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn scene_sound_descriptors_preserve_original_references() {
    let dir = test_game_dir();
    let engine = AudioEngine::new(&dir, false, false);
    let first = PuzzleAsset::load(&dir, 1, false).unwrap();
    let mut missing_music = Vec::new();
    assert_eq!(first.sounds[0].sounds.len(), 1);
    assert_eq!(first.sounds[0].sounds[0].stem, "SOUND0");
    assert_eq!(first.sounds[1].sounds[1].stem, "SFX-2");
    for num in 1..=15 {
        let asset = PuzzleAsset::load(&dir, num, false).unwrap();
        for entry in &asset.sounds {
            for desc in &entry.sounds {
                if desc.stem.is_empty() {
                    continue;
                }
                let phrase = desc.phrase_idx as usize;
                if desc.stem.starts_with("SOUND") {
                    if !engine.iss_songs.contains_key(&(desc.stem.clone(), phrase)) {
                        missing_music.push(format!("P{num:02}:{}:{phrase}", desc.stem));
                    }
                } else {
                    let bank = engine
                        .nnv_banks
                        .get(&desc.stem)
                        .unwrap_or_else(|| panic!("P{num:02}: missing {}", desc.stem));
                    assert!(
                        !synthesize_nnv_phrase(bank, phrase).is_empty(),
                        "P{num:02}: {}:{phrase}",
                        desc.stem
                    );
                }
            }
        }
    }
    assert_eq!(missing_music, ["P11:SOUND3:2"]);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn test_puzzle_cutting_all_difficulties_curved_and_straight() {
    let dir = test_game_dir();
    let asset = PuzzleAsset::load(&dir, 1, false).expect("P01.PZ8 should load");
    let tbx = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).expect("MJP.TBX should load");
    let font = FontAsset::load(&dir.join("MJP.NFT")).expect("MJP.NFT should load");

    for dim in 2..=8 {
        for &curved in &[false, true] {
            let sess = PuzzleSession::new(1, asset.clone(), dim, curved, true, true);
            assert_eq!(sess.pieces.len(), dim * dim);
            assert_eq!(sess.tray_order.len(), dim * dim);

            // Verify every cut piece has non-zero pixels
            for (i, p) in sess.pieces.iter().enumerate() {
                let non_zero = p.sprite.pixels.iter().filter(|&&px| px != 0).count();
                assert!(
                    non_zero > 20,
                    "Piece {} in dim={} curved={} has too few opaque pixels ({})",
                    i,
                    dim,
                    curved,
                    non_zero
                );
            }

            let mut fb = Framebuffer::new();
            let mut renderer = PuzzleRenderer::new();
            renderer.render(&sess, &mut fb, &tbx, &font, 160, 100);
        }
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn native_bubblegum_sound_remains_in_p11_scene_sequences() {
    let dir = test_game_dir();
    let audio = AudioEngine::new(&dir, false, false);
    assert_eq!(
        u16::from_le_bytes(audio.nnv_banks["BUBGUM"][..2].try_into().unwrap()),
        1
    );
    for ega in [false, true] {
        let asset = PuzzleAsset::load(&dir, 11, ega).unwrap();
        let index = asset
            .sounds
            .iter()
            .position(|entry| entry.tag == "BUBGUM")
            .unwrap();
        let entry = &asset.sounds[index];
        assert_eq!(entry.sounds[1].stem, "BUBGUM");
        assert_eq!(entry.sounds[1].phrase_idx, 0);
        assert!(entry.bubbles[0].is_none());
        for name in ["F_ALL", "F_TAPE"] {
            let sequence = asset
                .sequences
                .iter()
                .find(|sequence| sequence.name == name)
                .unwrap();
            assert!(asset.records[sequence.start_frame..sequence.end_frame].iter().any(|record| matches!(record,
                assets::SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                    matches!(command, assets::FrameCommand::Sound { snd_idx: Some(sound) } if *sound == index))
            )), "{name} ega={ega}");
        }
        let tape = asset
            .sequences
            .iter()
            .find(|sequence| sequence.name == "F_TAPE")
            .unwrap();
        assert!(asset.records[tape.start_frame..tape.end_frame].iter().any(
            |record| matches!(record,
                assets::SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                    matches!(command, assets::FrameCommand::Delay { ticks: 998, .. }))
            )
        ));
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn test_audio_decoders_nnv_ess_and_iss() {
    let dir = test_game_dir();
    let engine = AudioEngine::new(&dir, false, false);

    for key in ["BUBGUM", "PROG_0", "SFX-2", "P01", "P15"] {
        let bank = engine
            .nnv_banks
            .get(key)
            .unwrap_or_else(|| panic!("Missing NNV bank {}", key));
        let samples = synthesize_nnv_phrase(bank, 0);
        assert!(
            !samples.is_empty(),
            "NNV bank {} phrase 0 should produce samples",
            key
        );
    }

    for stem in ["TITLE", "SOUND0", "SOUND1", "SOUND2", "SOUND3"] {
        let song = engine
            .iss_songs
            .get(&(stem.to_string(), 0))
            .unwrap_or_else(|| panic!("Missing ISS song {}:0", stem));
        let samples = synthesize_iss_song(song);
        assert!(
            !samples.is_empty(),
            "ISS song {}:0 should produce samples",
            stem
        );
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn iss_music_and_standalone_speech_banks_load() {
    let engine = AudioEngine::new(&test_game_dir(), false, false);
    for stem in ["TITLE", "P01", "P15", "SP-231", "SFX-19"] {
        assert!(engine.nnv_banks.contains_key(stem), "{stem}");
    }
    for stem in ["TITLE", "SOUND0", "SOUND1", "SOUND2", "SOUND3"] {
        let song = engine.iss_songs.get(&(stem.to_string(), 0)).unwrap();
        assert!(!synthesize_iss_song(song).is_empty());
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn click_to_pick_place_complete_and_skip_animation() {
    let dir = test_game_dir();
    let mut audio = AudioEngine::new(&dir, false, false);
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    let mut completed = 0;
    let mut placed = 0;
    for ega in [false, true] {
        for num in 1..=15 {
            let asset = PuzzleAsset::load(&dir, num, ega).unwrap();
            let toolbox =
                ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega())).unwrap();
            for dim in 2..=8 {
                for curved in [false, true] {
                    let mut session =
                        PuzzleSession::new(num, asset.clone(), dim, curved, true, true);
                    for step in 0..dim * dim {
                        session.handle_pointer_press(20, 40, &mut audio);
                        let index = session.held_piece.expect("tray pick");
                        let piece = &session.pieces[index];
                        let x = session.playfield_x() + piece.base_x + piece.base_w / 2;
                        let y = session.playfield_y() + piece.base_y + piece.base_h / 2;
                        if step % 2 == 0 {
                            session.handle_pointer_release(21, 41, &mut audio);
                            assert_eq!(session.held_piece, Some(index));
                            session.handle_pointer_press(x, y, &mut audio);
                        }
                        session.handle_pointer_release(x, y, &mut audio);
                        assert!(
                            session.pieces[index].placed,
                            "P{num:02} ega={ega} grid={dim} curved={curved} piece={index}"
                        );
                        assert!(session.held_piece.is_none());
                        placed += 1;
                    }
                    assert!(session.solved);
                    assert!(
                        audio.common_caption().is_none(),
                        "stale pickup caption after P{num:02}"
                    );
                    assert!(session.tray_order.is_empty());
                    assert!(session.pieces.iter().all(|piece| piece.placed));
                    let mut fb = Framebuffer::new();
                    let mut renderer = PuzzleRenderer::new();
                    renderer.render(&session, &mut fb, &toolbox, &font, 160, 100);
                    assert!(session.active_anim.is_some());
                    session.handle_click(160, 100, &mut audio);
                    assert!(session.active_anim.is_none());
                    renderer.render(&session, &mut fb, &toolbox, &font, 160, 100);
                    assert_eq!(
                        session.handle_click(2, 2, &mut audio),
                        Some(PuzzleAction::OpenMenu)
                    );
                    completed += 1;
                }
            }
        }
    }
    assert_eq!(completed, 420);
    assert_eq!(placed, 12180);
    println!(
        "{completed} complete games; {placed} pieces placed through alternating click-carry and drag-release input"
    );
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn animation_preserves_elapsed_remainder() {
    let mut asset = PuzzleAsset::load(&test_game_dir(), 1, false).unwrap();
    asset.records = vec![
        assets::SceneRecord::Frame {
            bg_frame_idx: 0,
            bg_color: 0,
            commands: Vec::new(),
        };
        3
    ];
    let mut session = PuzzleSession::new(1, asset, 3, true, true, true);
    let mut audio = AudioEngine::new(&test_game_dir(), false, false);
    session.active_anim = Some(puzzle::AnimationPlayer {
        seq_idx: 0,
        start_frame: 0,
        previous_frame: 0,
        cur_frame: 0,
        end_frame: 3,
        tick_accum: 0.0,
        frame_delay_secs: 2.0 * 65536.0 / 1193182.0,
        repetitions_left: 1,
        default_delay_secs: 2.0 * 65536.0 / 1193182.0,
        transition_mode: 0,
        bubble: None,
        wait_for_audio: false,
        looping_sound: None,
    });
    let frame_secs = session.active_anim.as_ref().unwrap().frame_delay_secs;
    session.update(2.5 * frame_secs, &mut audio);
    let anim = session.active_anim.as_ref().unwrap();
    assert_eq!(anim.cur_frame, 2);
    assert!((anim.tick_accum - 0.5 * frame_secs).abs() < 0.00001);
    let anim = session.active_anim.as_mut().unwrap();
    anim.cur_frame = 0;
    anim.tick_accum = 0.0;
    anim.repetitions_left = 2;
    session.update(4.5 * frame_secs, &mut audio);
    let anim = session.active_anim.as_ref().unwrap();
    assert_eq!(anim.cur_frame, 1);
    assert_eq!(anim.repetitions_left, 1);
    assert!((anim.tick_accum - 0.5 * frame_secs).abs() < 0.00001);
}

#[test]
fn polygon_clips_full_signed_asset_coordinates() {
    let mut fb = Framebuffer::new();
    fb.fill_polygon(&[(-32768, -32768), (32767, 32767), (-32768, 32767)], 15);
    assert_eq!(fb.pixels[199 * 320 + 199], 15);
    assert_eq!(fb.pixels[199 * 320 + 200], 0);
}

#[test]
fn setup_reports_save_failure_and_ignores_outside_clicks() {
    let mut setup = SetupMenu::new(Preferences::default());
    let original_text = setup.prefs.text_msg_on;
    setup.selected_row = 5;
    assert!(!setup.activate_or_cycle(Path::new("/dev/null")));
    assert_eq!(setup.prefs.text_msg_on, original_text ^ 1);
    setup.selected_row = setup.num_rows() - 1;
    assert!(!setup.activate_or_cycle(Path::new("/dev/null")));
    assert!(setup.error.is_some());
    let before = setup.prefs;
    setup.handle_click(-1, 32, Path::new("/dev/null"));
    assert_eq!(setup.prefs, before);
}

#[test]
fn thumbnails_fit_inside_their_tray_slot() {
    let mut fb = Framebuffer::new();
    let sprite = assets::Sprite8061 {
        stride: 135,
        height: 98,
        transparent_color: 248,
        opacity: None,
        bpp: 8,
        sub_x: 0,
        sub_y: 0,
        width: 135,
        sub_h: 98,
        pixels: vec![7; 135 * 98],
    };
    fb.blit_thumbnail(&sprite, 3, 32, 40, 48);
    let mut count = 0;
    for y in 0..200 {
        for x in 0..320 {
            if fb.get_pixel(x, y) == 7 {
                assert!((3..43).contains(&x) && (32..80).contains(&y));
                count += 1;
            }
        }
    }
    assert!(count > 1000);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn toolbox_graphics_match_actual_scene_depth_including_fallbacks() {
    let mut app = test_browser();
    app.setup_menu.prefs.video_mode = 2;
    app.sync_prefs_from_setup();
    for id in [1, 6, 7, 10] {
        app.selected_puzzle = id;
        app.restart_selected_puzzle();
        app.audio.update(60.0);
        let toolbox = ToolboxAsset::load(
            &app.game_dir.join("MJP.TBX"),
            usize::from(app.session.as_ref().unwrap().asset.sprites[0].bpp == 4),
        )
        .unwrap();
        let mut expected = Framebuffer::new();
        let mut puzzle_renderer = PuzzleRenderer::new();
        puzzle_renderer.render(
            app.session.as_ref().unwrap(),
            &mut expected,
            &toolbox,
            &app.font,
            160,
            100,
        );
        let mut app_renderer = AppRenderer::new();
        app_renderer.render(&app, 160, 100);
        let actual = &app_renderer.fb;
        assert!(
            actual.pixels == expected.pixels,
            "P{id:02}, toolbox variant {}",
            usize::from(app.session.as_ref().unwrap().asset.sprites[0].bpp == 4)
        );
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_and_puzzle_controls_use_palette_colors() {
    let dir = test_game_dir();
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        let asset = PuzzleAsset::load(&dir, 1, ega).unwrap();
        let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(ega)).unwrap();
        let session = PuzzleSession::new(1, asset, 3, true, true, true);
        let mut puzzle_renderer = PuzzleRenderer::new();
        let mut fb = Framebuffer::new();
        puzzle_renderer.render(&session, &mut fb, &toolbox, &font, 160, 100);
        assert_eq!(fb.get_pixel(1, 100), fb.color_index([0; 3]), "ega={ega}");
        assert_eq!(fb.get_pixel(3, 19), fb.color_index([85; 3]), "ega={ega}");
        assert_ne!(fb.get_pixel(1, 100), fb.get_pixel(3, 19), "ega={ega}");
        let mut app = test_browser();
        app.setup_menu.prefs.video_mode = if ega { 2 } else { 1 };
        app.sync_prefs_from_setup();
        let mut app_renderer = AppRenderer::new();
        app_renderer.render(&app, 160, 100);
        let fb = &app_renderer.fb;
        assert_eq!(fb.get_pixel(38, 6), fb.color_index([0; 3]), "ega={ega}");
        assert_eq!(fb.get_pixel(36, 4), fb.color_index([255; 3]), "ega={ega}");
        assert_ne!(fb.get_pixel(38, 6), fb.get_pixel(36, 4), "ega={ega}");
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn ui_white_uses_the_loaded_palette() {
    let asset = PuzzleAsset::load(&test_game_dir(), 1, false).unwrap();
    let mut fb = Framebuffer::new();
    fb.palette = asset.palette;
    let white = fb.color_index([255; 3]);
    assert_eq!(fb.palette[white as usize], [255; 3]);
    assert_ne!(white, 15);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn printable_export_preserves_bitmap_and_reports_write_failure() {
    let temp = env::temp_dir().join(format!("mickey-print-test-{}", std::process::id()));
    std::fs::create_dir(&temp).unwrap();
    let source = test_game_dir();
    std::fs::copy(source.join("P01.PRT"), temp.join("P01.PRT")).unwrap();
    let asset = PuzzleAsset::load(&source, 1, false).unwrap();
    let mut session = PuzzleSession::new(1, asset, 3, true, true, true);
    session.export_printable_picture(&temp);
    let image = std::fs::read(temp.join("prints/P01.pbm")).unwrap();
    assert_eq!(&image[..11], b"P4\n320 200\n");
    assert_eq!(&image[11..], &PrintAsset::load(&source, 1).unwrap().bitmap);
    std::fs::remove_dir_all(temp.join("prints")).unwrap();
    std::fs::write(temp.join("prints"), b"blocked").unwrap();
    session.export_printable_picture(&temp);
    assert!(
        session
            .status_message
            .as_ref()
            .unwrap()
            .0
            .starts_with("Print error:")
    );
    std::fs::remove_dir_all(temp).unwrap();
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_navigation_wraps_and_buttons_align() {
    let mut app = GameApp::new(test_game_dir(), true).unwrap();
    app.screen = AppScreen::Browser;
    app.selected_puzzle = 1;
    app.step_puzzle(-1);
    assert_eq!(app.selected_puzzle, 15);
    app.step_puzzle(1);
    assert_eq!(app.selected_puzzle, 1);

    app.selected_grid_dim = 1;
    app.step_grid_dim(-1);
    assert_eq!(app.selected_grid_dim, 8);
    app.step_grid_dim(1);
    assert_eq!(app.selected_grid_dim, 1);

    app.selected_grid_dim = 3;
    let edges_was = app.curved_edges;
    let edges_btn = BROWSER_BUTTONS[3];
    app.handle_browser_click(edges_btn.x + edges_btn.w / 2, edges_btn.y + edges_btn.h / 2);
    assert_ne!(app.curved_edges, edges_was);

    let setup_btn = BROWSER_BUTTONS[4];
    app.handle_browser_click(setup_btn.x + setup_btn.w / 2, setup_btn.y + setup_btn.h / 2);
    assert_eq!(
        app.screen,
        AppScreen::Setup {
            return_to_playing: false
        }
    );
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn open_setup_clears_previous_error() {
    let mut app = test_browser();
    app.setup_menu.error = Some("stale error".to_string());
    app.open_setup(false);
    assert!(app.setup_menu.error.is_none());
    assert_eq!(
        app.screen,
        AppScreen::Setup {
            return_to_playing: false
        }
    );
}

#[test]
#[ignore]
fn perf_benchmark() {
    use std::hint::black_box;
    use std::time::Instant;

    let dir = test_game_dir();
    let t0 = Instant::now();
    for id in 1..=15 {
        black_box(PuzzleAsset::load(&dir, id, false).unwrap());
    }
    let load_all_vga = t0.elapsed();

    let t0_ega = Instant::now();
    for id in 1..=15 {
        black_box(PuzzleAsset::load(&dir, id, true).unwrap());
    }
    let load_all_ega = t0_ega.elapsed();

    let asset = PuzzleAsset::load(&dir, 1, false).unwrap();
    let t1 = Instant::now();
    let mut session = PuzzleSession::new(1, asset.clone(), 8, true, true, true);
    let cut_8x8 = t1.elapsed();

    let t1_preset = Instant::now();
    let mut preset_session = PuzzleSession::new(1, asset.clone(), 1, true, true, true);
    let cut_preset = t1_preset.elapsed();

    let tbx = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).unwrap();
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    let mut renderer = PuzzleRenderer::new();
    let mut preset_renderer = PuzzleRenderer::new();
    let mut fb = Framebuffer::new();
    let mut rgba = vec![0u8; 320 * 200 * 4];

    // Warm up caches
    renderer.render(&session, &mut fb, &tbx, &font, 160, 100);
    fb.to_rgba8(&mut rgba);
    renderer.blend_picture_guide(&session, &mut rgba, 160, 100);

    let iters = 2000;
    let t2 = Instant::now();
    for _ in 0..iters {
        renderer.render(&session, &mut fb, &tbx, &font, 160, 100);
        black_box(&fb.pixels[0]);
    }
    let render_warm = t2.elapsed();

    // Dragging a piece
    session.held_piece = Some(0);
    let t_drag = Instant::now();
    for i in 0..iters {
        let mx = 80 + (i % 160);
        let my = 40 + (i % 120);
        renderer.render(&session, &mut fb, &tbx, &font, mx, my);
        fb.to_rgba8(&mut rgba);
        renderer.blend_picture_guide(&session, &mut rgba, mx, my);
        black_box(rgba[0]);
    }
    let drag_full = t_drag.elapsed();
    session.held_piece = None;

    let t3 = Instant::now();
    for _ in 0..iters {
        fb.to_rgba8(&mut rgba);
        black_box(rgba[0]);
    }
    let to_rgba = t3.elapsed();

    let t4 = Instant::now();
    for _ in 0..iters {
        renderer.blend_picture_guide(&session, &mut rgba, 160, 100);
        black_box(rgba[0]);
    }
    let guide = t4.elapsed();

    // Solved idle & animation/dissolve
    let mut audio = AudioEngine::new(&dir, false, false);
    preset_session.auto_solve(&mut audio);
    let anim_iters = 500;
    let t_anim = Instant::now();
    for _ in 0..anim_iters {
        preset_renderer.render(&preset_session, &mut fb, &tbx, &font, 160, 100);
        black_box(&fb.pixels[0]);
    }
    let render_anim = t_anim.elapsed();

    if let Some(anim) = preset_session.active_anim.as_mut() {
        anim.transition_mode = 8;
        anim.tick_accum = anim.frame_delay_secs * 0.25;
    }
    let t_dissolve = Instant::now();
    for _ in 0..anim_iters {
        preset_renderer.render(&preset_session, &mut fb, &tbx, &font, 160, 100);
        black_box(&fb.pixels[0]);
    }
    let render_dissolve = t_dissolve.elapsed();

    // Dragging a piece with the new layer-composited pipeline:
    // 1) CPU-only cache-hit cost (offset/key lookup when board/toolbox/held layers are cached)
    // 2) Actual headless SDL3 software-renderer Canvas (clear + 3x texture copy + present)
    session.held_piece = Some(0);
    let mut layer_board = vec![0u8; 320 * 200 * 4];
    let mut layer_tbx = Vec::new();
    let mut layer_held = Vec::new();
    renderer.render_board_layer_rgba(&session, &tbx, true, &mut layer_board);
    let tbx_bounds =
        renderer.render_toolbox_layer_rgba(&session, &tbx, &font, 160, 100, &mut layer_tbx);
    let held_bounds = renderer
        .render_held_group_layer_rgba(&session, 160, 100, &mut layer_held)
        .unwrap();
    let t_drag_cache_hit = Instant::now();
    for i in 0..iters {
        let mx = 80 + (i % 160);
        let my = 40 + (i % 120);
        let anchor_cell = session.held_group_anchor_cell(mx, my);
        let anchor = &session.pieces[0];
        let ax = mx - anchor.base_w / 2 + held_bounds.0;
        let ay = my - anchor.base_h / 2 + held_bounds.1;
        black_box((
            ax,
            ay,
            anchor_cell,
            &layer_board[0],
            &layer_tbx[0],
            &layer_held[0],
        ));
    }
    let drag_cpu_cache_hit = t_drag_cache_hit.elapsed();

    let mut sw_canvas = sdl3::surface::Surface::new(960, 720, sdl3::pixels::PixelFormat::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    let sw_creator = sw_canvas.texture_creator();
    let mut sw_board_tex = sw_creator
        .create_texture_static(sdl3::pixels::PixelFormat::RGBA32, 320, 200)
        .unwrap();
    sw_board_tex.update(None, &layer_board, 320 * 4).unwrap();
    let mut sw_tbx_tex = sw_creator
        .create_texture_static(
            sdl3::pixels::PixelFormat::RGBA32,
            tbx_bounds.2 as u32,
            tbx_bounds.3 as u32,
        )
        .unwrap();
    sw_tbx_tex
        .update(None, &layer_tbx, (tbx_bounds.2 * 4) as usize)
        .unwrap();
    let mut sw_held_tex = sw_creator
        .create_texture_static(
            sdl3::pixels::PixelFormat::RGBA32,
            held_bounds.2 as u32,
            held_bounds.3 as u32,
        )
        .unwrap();
    sw_held_tex
        .update(None, &layer_held, (held_bounds.2 * 4) as usize)
        .unwrap();
    let sw_iters = 200;
    let t_sdl_sw = Instant::now();
    for i in 0..sw_iters {
        let mx = 80 + (i % 160);
        let my = 40 + (i % 120);
        let anchor = &session.pieces[0];
        let ax = (mx - anchor.base_w / 2 + held_bounds.0) as f32 * 3.0;
        let ay = (my - anchor.base_h / 2 + held_bounds.1) as f32 * 3.0;
        sw_canvas.clear();
        sw_canvas
            .copy(
                &sw_board_tex,
                None,
                Some(sdl3::render::FRect::new(0.0, 60.0, 960.0, 600.0)),
            )
            .unwrap();
        sw_canvas
            .copy(
                &sw_tbx_tex,
                None,
                Some(sdl3::render::FRect::new(
                    tbx_bounds.0 as f32 * 3.0,
                    60.0 + tbx_bounds.1 as f32 * 3.0,
                    tbx_bounds.2 as f32 * 3.0,
                    tbx_bounds.3 as f32 * 3.0,
                )),
            )
            .unwrap();
        sw_canvas
            .copy(
                &sw_held_tex,
                None,
                Some(sdl3::render::FRect::new(
                    ax,
                    60.0 + ay,
                    held_bounds.2 as f32 * 3.0,
                    held_bounds.3 as f32 * 3.0,
                )),
            )
            .unwrap();
        sw_canvas.present();
    }
    let drag_sdl_sw_canvas = t_sdl_sw.elapsed();
    session.held_piece = None;

    eprintln!(
        "load_15_vga={:.2}ms load_15_ega={:.2}ms cut_8x8={:.2}ms cut_preset={:.2}ms \
         render_warm={:.2}us/frame drag_full={:.2}us/frame drag_cpu_cache_hit={:.3}us/frame \
         drag_sdl_sw_canvas={:.2}us/frame to_rgba8={:.2}us/frame guide={:.2}us/frame \
         render_anim={:.2}us/frame render_dissolve={:.2}us/frame",
        load_all_vga.as_secs_f64() * 1000.0,
        load_all_ega.as_secs_f64() * 1000.0,
        cut_8x8.as_secs_f64() * 1000.0,
        cut_preset.as_secs_f64() * 1000.0,
        render_warm.as_secs_f64() * 1e6 / iters as f64,
        drag_full.as_secs_f64() * 1e6 / iters as f64,
        drag_cpu_cache_hit.as_secs_f64() * 1e6 / iters as f64,
        drag_sdl_sw_canvas.as_secs_f64() * 1e6 / sw_iters as f64,
        to_rgba.as_secs_f64() * 1e6 / iters as f64,
        guide.as_secs_f64() * 1e6 / iters as f64,
        render_anim.as_secs_f64() * 1e6 / anim_iters as f64,
        render_dissolve.as_secs_f64() * 1e6 / anim_iters as f64,
    );
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn present_signature_skips_idle_board_motion_and_tracks_visual_changes() {
    let dir = test_game_dir();
    let asset = PuzzleAsset::load(&dir, 1, false).unwrap();
    let mut session = PuzzleSession::new(1, asset, 4, true, true, true);

    // Moving the cursor around the board without a held piece does not change the signature
    let idle_a = session.present_key(120, 80, true);
    let idle_b = session.present_key(200, 140, true);
    assert_eq!(idle_a, idle_b);

    // Moving over a tray slot changes the signature (hover highlight)
    let tray_hover = session.present_key(20, 40, true);
    assert_ne!(idle_a, tray_hover);

    // Toggling picture guide changes the signature
    let no_guide = session.present_key(120, 80, false);
    assert_ne!(idle_a, no_guide);

    // Holding a piece tracks cursor position changes
    session.held_piece = Some(0);
    let held_a = session.present_key(120, 80, true);
    let held_b = session.present_key(124, 80, true);
    assert_ne!(idle_a, held_a);
    assert_ne!(held_a, held_b);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_present_signature_tracks_and_expires_common_captions() {
    let dir = test_game_dir();
    let mut app = GameApp::new(dir, true).unwrap();
    app.screen = AppScreen::Browser;
    app.prefs.text_msg_on = 1;

    let idle_sig = app.present_signature(120, 80, false, false);
    assert!(idle_sig.is_some());

    app.audio.play_common(CommonSound::NextPicture);
    let next_sig = app.present_signature(120, 80, false, false);
    assert_ne!(idle_sig, next_sig);

    app.audio.play_common(CommonSound::StartPuzzle);
    let start_sig = app.present_signature(120, 80, false, false);
    assert_ne!(next_sig, start_sig);

    // Advance enough time for the common caption to expire on an idle Browser screen
    for _ in 0..20 {
        app.audio.update(0.5);
    }
    assert!(app.audio.common_caption().is_none());
    let expired_sig = app.present_signature(120, 80, false, false);
    assert_eq!(expired_sig, idle_sig);
}

fn synthetic_app() -> GameApp {
    use crate::assets::{FrameCommand, NamedSequence, SceneRecord, SequencePlayback, Sprite8061};
    let mut palette = [[0u8; 3]; 256];
    palette[1] = [0, 0, 170];
    palette[7] = [204, 204, 187];
    palette[15] = [255, 255, 255];
    let sprite = Sprite8061 {
        stride: 274,
        height: 200,
        transparent_color: 0,
        bpp: 8,
        sub_x: 0,
        sub_y: 0,
        width: 274,
        sub_h: 200,
        pixels: vec![1; 274 * 200],
        opacity: None,
    };
    let preview = PuzzleAsset {
        id: 1,
        palette,
        records: vec![
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
        ],
        sequences: vec![NamedSequence {
            name: "FILM".to_string(),
            header_idx: 1,
            start_frame: 0,
            end_frame: 1,
        }],
        hotspots: Vec::new(),
        full_anim_seq_idx: 0,
        sounds: Vec::new(),
        sprites: vec![sprite],
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
    let tbx = ToolboxAsset {
        palette,
        sprites: Vec::new(),
    };
    let prefs = Preferences {
        music_on: 0,
        sfx_on: 0,
        text_msg_on: 1,
        ..Preferences::default()
    };
    GameApp::from_parts(
        PathBuf::from("nonexistent_test_dir"),
        font,
        tbx.clone(),
        tbx,
        preview,
        prefs,
        false,
    )
}

#[test]
fn app_state_transitions_render_cache_and_captions_without_assets() {
    let mut app = synthetic_app();
    let mut renderer = AppRenderer::new();

    // Start in Title -> Enter advances to Browser (since credit_images is empty)
    app.set_screen(AppScreen::Title);
    assert!(renderer.render_if_needed(&app, 100, 100, false, true));
    assert!(!renderer.render_if_needed(&app, 120, 100, false, true));

    app.handle_input(&AppInput {
        enter: true,
        ..AppInput::default()
    });
    assert_eq!(app.screen, AppScreen::Browser);
    assert!(renderer.render_if_needed(&app, 100, 100, false, true));
    assert!(!renderer.render_if_needed(&app, 140, 120, false, true));

    // Common caption invalidates Browser signature and expires back to idle signature
    let idle_sig = app.present_signature(100, 100, false, false);
    app.audio.play_common(CommonSound::NextPicture);
    assert_ne!(app.present_signature(100, 100, false, false), idle_sig);
    assert!(renderer.render_if_needed(&app, 100, 100, false, false));

    for _ in 0..20 {
        app.handle_input(&AppInput {
            dt: 0.5,
            ..AppInput::default()
        });
    }
    assert!(app.audio.common_caption().is_none());
    assert_eq!(app.present_signature(100, 100, false, false), idle_sig);
    assert!(renderer.render_if_needed(&app, 100, 100, false, false));

    // Enter Playing with a synthetic session, then F4 pauses into Setup, Esc resumes Playing
    app.session = Some(PuzzleSession::new(
        1,
        app.preview_asset.clone(),
        2,
        true,
        false,
        true,
    ));
    app.set_screen(AppScreen::Playing);
    assert!(renderer.render_if_needed(&app, 100, 100, false, true));
    assert!(!renderer.render_if_needed(&app, 150, 120, false, true));

    app.handle_input(&AppInput {
        f4: true,
        ..AppInput::default()
    });
    assert_eq!(
        app.screen,
        AppScreen::Setup {
            return_to_playing: true
        }
    );
    assert!(renderer.render_if_needed(&app, 100, 100, false, true));

    // Setup menu navigation updates revision and redraws
    app.handle_input(&AppInput {
        down: true,
        ..AppInput::default()
    });
    assert_eq!(app.setup_menu.selected_row, 1);
    assert!(renderer.render_if_needed(&app, 100, 100, false, true));

    // Esc exits Setup back to Playing
    app.handle_input(&AppInput {
        escape: true,
        ..AppInput::default()
    });
    assert_eq!(app.screen, AppScreen::Playing);
    assert!(renderer.render_if_needed(&app, 100, 100, false, true));
}

#[test]
fn viewport_resizing_preserves_aspect_ratio_across_desktop_and_compact_sizes() {
    for (w, h, playing) in [
        (960.0, 720.0, true),
        (1280.0, 720.0, true),
        (1700.0, 1380.0, true),
        (400.0, 300.0, true),
        (320.0, 200.0, false),
    ] {
        let vp = game_viewport(w, h, playing);
        assert!(vp.x >= 0.0 && vp.y >= 0.0);
        assert!(vp.x + vp.w <= w + 0.01);
        assert!(vp.y + vp.h <= h + 0.01);
        assert!((vp.w / vp.h - 1.6).abs() < 0.001);
    }
}

#[test]
fn reference_signature_invalidates_across_replaced_sessions_and_assets_without_assets() {
    let mut app = synthetic_app();
    let mut renderer = AppRenderer::new();

    app.session = Some(PuzzleSession::new(
        1,
        app.preview_asset.clone(),
        2,
        true,
        false,
        true,
    ));
    app.set_screen(AppScreen::Playing);
    let sig1 = app.present_signature(100, 100, true, false);
    assert!(renderer.render_if_needed(&app, 100, 100, true, false));
    assert!(!renderer.render_if_needed(&app, 120, 120, true, false));

    // Replacing the session with the same puzzle_num and video mode must invalidate Reference.
    app.session = Some(PuzzleSession::new(
        1,
        app.preview_asset.clone(),
        3,
        false,
        false,
        true,
    ));
    let sig2 = app.present_signature(100, 100, true, false);
    assert_ne!(sig2, sig1);
    assert!(renderer.render_if_needed(&app, 100, 100, true, false));

    // Replacing the scene asset on the same session must also invalidate Reference.
    let mut alt_asset = app.preview_asset.clone();
    alt_asset.palette[1] = [200, 100, 50];
    app.session.as_mut().unwrap().replace_scene_asset(alt_asset);
    let sig3 = app.present_signature(100, 100, true, false);
    assert_ne!(sig3, sig2);
    assert!(renderer.render_if_needed(&app, 100, 100, true, false));

    let mut fresh = AppRenderer::new();
    assert!(fresh.render_if_needed(&app, 100, 100, true, false));
    assert_eq!(renderer.rgba_buf, fresh.rgba_buf);
}

#[test]
fn title_escape_skips_directly_to_browser() {
    let mut app = synthetic_app();
    app.screen = AppScreen::Title;
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });
    assert_eq!(app.screen, AppScreen::Browser);
}

#[test]
fn playing_escape_exits_without_updating_session_on_exit_frame() {
    let mut app = synthetic_app();
    app.session = Some(PuzzleSession::new(
        1,
        app.preview_asset.clone(),
        2,
        true,
        false,
        true,
    ));
    app.set_screen(AppScreen::Playing);

    app.handle_input(&AppInput {
        dt: 1.0,
        escape: true,
        f2: true,
        ..Default::default()
    });

    assert_eq!(app.screen, AppScreen::Browser);
    let session = app.session.as_ref().unwrap();
    assert!(!session.solved);
    assert_eq!(session.elapsed_secs, 0.0);
}

#[test]
fn title_and_credits_ignore_non_finite_and_negative_dt() {
    let mut app = synthetic_app();
    app.screen = AppScreen::Title;
    for dt in [f32::NAN, f32::NEG_INFINITY, -5.0] {
        app.handle_input(&AppInput {
            dt,
            ..Default::default()
        });
        assert_eq!(app.screen, AppScreen::Title);
    }
    app.handle_input(&AppInput {
        dt: 8.1,
        ..Default::default()
    });
    assert_eq!(app.screen, AppScreen::Browser);

    app.set_screen(AppScreen::Credits { page: 0 });
    for dt in [f32::NAN, f32::NEG_INFINITY, -5.0] {
        app.handle_input(&AppInput {
            dt,
            ..Default::default()
        });
        assert_eq!(app.screen, AppScreen::Credits { page: 0 });
    }
    app.handle_input(&AppInput {
        dt: 5.1,
        ..Default::default()
    });
    assert_eq!(app.screen, AppScreen::Browser);
}

#[test]
fn enter_on_frame_animation_completes_naturally_does_not_restart_or_spawn_pointer() {
    let mut app = synthetic_app();
    let mut session = PuzzleSession::new(1, app.preview_asset.clone(), 2, true, false, false);
    session.auto_solve(&mut app.audio);
    assert!(session.active_anim.is_some());
    assert!(session.keyboard_pointer.is_none());
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    app.handle_input(&AppInput {
        dt: 10.0,
        enter: true,
        ..Default::default()
    });

    assert_eq!(app.screen, AppScreen::Playing);
    let session = app.session.as_ref().unwrap();
    assert!(session.active_anim.is_none());
    assert!(session.keyboard_pointer.is_none());
}

#[test]
fn setup_left_on_save_and_return_does_not_activate_save() {
    let mut app = synthetic_app();
    app.open_setup(false);
    app.setup_menu.selected_row = 6;
    app.handle_input(&AppInput {
        left: true,
        ..Default::default()
    });
    assert_eq!(
        app.screen,
        AppScreen::Setup {
            return_to_playing: false
        }
    );
    assert!(app.setup_menu.error.is_none());
}

#[test]
fn all_15_common_feedback_cues_play_and_set_expected_captions() {
    let mut audio = AudioEngine::new(Path::new("."), false, false);
    for sfx_only in [
        CommonSound::Cancel,
        CommonSound::SetupStart,
        CommonSound::Quit,
        CommonSound::UiClick,
    ] {
        audio.play_common(sfx_only);
        assert!(audio.common_caption().is_none());
    }
    for (speech, expected_substr) in [
        (CommonSound::NextPicture, "next picture"),
        (CommonSound::PreviousPicture, "last picture"),
        (CommonSound::StartPuzzle, "make this one"),
        (CommonSound::ChooseSize, "how many pieces"),
        (CommonSound::ChoosePiece, "choose a piece"),
        (CommonSound::ReturnPiece, "put this one back"),
        (CommonSound::OtherPicture, "different"),
        (CommonSound::ChangePieceSize, "different piece size"),
        (CommonSound::SeeWhatHappens, "see what happens"),
        (CommonSound::DoneWithThisOne, "done with this one"),
        (CommonSound::NotDoneYet, "not done yet"),
    ] {
        audio.play_common(speech);
        let caption = audio.common_caption().expect("speech caption");
        assert!(
            caption.contains(expected_substr),
            "unexpected caption {caption:?} for {speech:?}"
        );
    }
}

#[test]
fn unfinished_puzzle_exit_prompts_confirmation_and_handles_cancel_or_confirm() {
    let mut app = synthetic_app();
    let mut session = PuzzleSession::new(1, app.preview_asset.clone(), 2, true, false, false);
    session.pieces[0].placed = true;
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    // First Escape asks for confirmation instead of discarding progress.
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });
    assert_eq!(app.screen, AppScreen::Playing);
    assert!(app.confirm_exit_puzzle);
    assert!(
        app.session
            .as_ref()
            .unwrap()
            .status_message()
            .is_some_and(|m| m.contains("We're not done yet"))
    );

    // Second Escape cancels the confirmation and resumes playing.
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });
    assert_eq!(app.screen, AppScreen::Playing);
    assert!(!app.confirm_exit_puzzle);

    // Escape then Enter confirms exiting to Browser.
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(app.confirm_exit_puzzle);
    app.handle_input(&AppInput {
        enter: true,
        ..Default::default()
    });
    assert_eq!(app.screen, AppScreen::Browser);
    assert!(!app.confirm_exit_puzzle);
}

#[test]
fn in_game_hotkeys_toggle_music_sfx_timer_and_text_messages() {
    let mut app = synthetic_app();
    let session = PuzzleSession::new(1, app.preview_asset.clone(), 2, true, false, true);
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    let init_music = app.prefs.music_on;
    let init_sfx = app.prefs.sfx_on;
    let init_timer = app.prefs.timer_on;
    let init_text = app.prefs.text_msg_on;

    app.handle_input(&AppInput {
        f7: true,
        f8: true,
        key_t: true,
        key_m: true,
        ..Default::default()
    });

    assert_ne!(app.prefs.music_on, init_music);
    assert_ne!(app.prefs.sfx_on, init_sfx);
    assert_ne!(app.prefs.timer_on, init_timer);
    assert_ne!(app.prefs.text_msg_on, init_text);
    let sess = app.session.as_ref().unwrap();
    assert_eq!(sess.timer_enabled, app.prefs.timer_on != 0);
    assert_eq!(sess.text_enabled, app.prefs.text_msg_on != 0);
}

#[test]
fn f1_during_solved_animation_does_not_play_hint_cue() {
    let mut app = synthetic_app();
    let mut session = PuzzleSession::new(1, app.preview_asset.clone(), 2, true, false, false);
    session.auto_solve(&mut app.audio);
    assert!(session.solved);
    assert!(session.active_anim.is_some());
    assert!(app.audio.common_caption().is_none());

    app.session = Some(session);
    app.set_screen(AppScreen::Playing);
    app.handle_input(&AppInput {
        f1: true,
        ..Default::default()
    });

    assert!(app.audio.common_caption().is_none());
}

#[test]
fn escape_prompts_confirmation_when_all_placed_pieces_are_held_as_group() {
    let mut app = synthetic_app();
    let mut session = PuzzleSession::new(1, app.preview_asset.clone(), 4, false, false, false);

    // Place 2 adjacent pieces (0, 1)=1 and (1, 1)=5, then right-click to pick up the entire cluster.
    for (pidx, r, c) in [(1usize, 0usize, 1usize), (5, 1, 1)] {
        let pos = session.tray_order.iter().position(|&p| p == pidx).unwrap();
        session.tray_scroll = pos;
        session.handle_click(20, 40, &mut app.audio);
        let col_w = crate::puzzle::grid_col_widths(4);
        let row_h = crate::puzzle::grid_row_heights(4);
        let cx = session.playfield_x() + col_w[..c].iter().sum::<i32>() + col_w[c] / 2;
        let cy = session.playfield_y() + row_h[..r].iter().sum::<i32>() + row_h[r] / 2;
        session.handle_click(cx, cy, &mut app.audio);
    }
    let col_w = crate::puzzle::grid_col_widths(4);
    let row_h = crate::puzzle::grid_row_heights(4);
    let x01 = session.playfield_x() + col_w[0] + col_w[1] / 2;
    let y01 = session.playfield_y() + row_h[0] / 2;
    assert!(session.handle_right_click(x01, y01, &mut app.audio));
    assert_eq!(session.held_group.len(), 2);
    assert!(session.pieces.iter().all(|p| !p.placed));

    app.session = Some(session);
    app.set_screen(AppScreen::Playing);
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });

    assert_eq!(app.screen, AppScreen::Playing);
    assert!(app.confirm_exit_puzzle);
    assert!(
        app.session
            .as_ref()
            .unwrap()
            .status_message()
            .is_some_and(|m| m.contains("We're not done yet"))
    );
}

#[test]
fn browser_quit_and_print_buttons_and_hotkeys() {
    let mut app = synthetic_app();
    app.set_screen(AppScreen::Browser);
    assert!(!app.quit_requested);

    // Clicking Print button (index 6) sets browser_status.
    let print_btn = crate::app::BROWSER_BUTTONS[6];
    assert_eq!(print_btn.action, crate::app::BrowserAction::PrintPuzzle);
    app.handle_input(&AppInput {
        left_pressed: true,
        pointer_x: print_btn.x + print_btn.w / 2,
        pointer_y: print_btn.y + print_btn.h / 2,
        ..Default::default()
    });
    assert!(app.browser_status.is_some());

    // Pressing P triggers PrintPuzzle as well.
    app.browser_status = None;
    app.handle_input(&AppInput {
        key_p: true,
        ..Default::default()
    });
    assert!(app.browser_status.is_some());

    // Clicking Quit button (index 7) requests quit.
    let quit_btn = crate::app::BROWSER_BUTTONS[7];
    assert_eq!(quit_btn.action, crate::app::BrowserAction::QuitGame);
    app.handle_input(&AppInput {
        left_pressed: true,
        pointer_x: quit_btn.x + quit_btn.w / 2,
        pointer_y: quit_btn.y + quit_btn.h / 2,
        ..Default::default()
    });
    assert!(app.quit_requested);

    // Pressing Q or Escape in Browser also sets quit_requested.
    app.quit_requested = false;
    app.handle_input(&AppInput {
        key_q: true,
        ..Default::default()
    });
    assert!(app.quit_requested);

    app.quit_requested = false;
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(app.quit_requested);
}

#[test]
fn mickey_cursor_overlay_draws_without_invalidating_base_cache() {
    let mut app = synthetic_app();
    app.set_screen(AppScreen::Browser);

    // Populate synthetic sprites up to index 64 (62=pointing hand, 63=grab hand, 64=open hand)
    // with a solid white pixel at (0,0). Hotspot for 62 is (-6, 0).
    let mut palette = [[0u8; 3]; 256];
    palette[1] = [255, 250, 240];
    app.tbx.palette = palette;
    while app.tbx.sprites.len() <= 64 {
        app.tbx.sprites.push(crate::assets::Sprite8061::from_8bpp(
            2,
            2,
            0,
            vec![1, 0, 0, 0],
            vec![true, false, false, false],
        ));
    }
    assert!(app.has_mickey_cursor());

    let mut renderer = AppRenderer::new();
    assert!(renderer.render_if_needed(&app, 100, 80, false, false));
    let off_100 = (80 * 320 + (100 - 6)) * 4;
    assert_eq!(
        &renderer.rgba_buf[off_100..off_100 + 4],
        &[255, 250, 240, 255]
    );

    // Same pointer and state -> no re-render needed.
    assert!(!renderer.render_if_needed(&app, 100, 80, false, false));

    // Moving pointer updates cursor pixel at (140 - 6, 90) and restores (94, 80) from base cache.
    assert!(renderer.render_if_needed(&app, 140, 90, false, false));
    let off_140 = (90 * 320 + (140 - 6)) * 4;
    assert_eq!(
        &renderer.rgba_buf[off_140..off_140 + 4],
        &[255, 250, 240, 255]
    );
    assert_ne!(
        &renderer.rgba_buf[off_100..off_100 + 4],
        &[255, 250, 240, 255]
    );
}

#[test]
fn wasd_then_mouse_motion_restores_pointer_and_click_target() {
    let mut app = synthetic_app();
    let mut palette = [[0u8; 3]; 256];
    palette[1] = [255, 250, 240];
    app.tbx.palette = palette;
    while app.tbx.sprites.len() <= 64 {
        app.tbx.sprites.push(crate::assets::Sprite8061::from_8bpp(
            2,
            2,
            0,
            vec![1, 0, 0, 0],
            vec![true, false, false, false],
        ));
    }

    let session = PuzzleSession::new(1, app.preview_asset.clone(), 4, false, false, false);
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    // 1. WASD steers keyboard_pointer away from physical mouse coordinates (150, 120).
    app.handle_input(&AppInput {
        dt: 0.25,
        move_x: 1,
        pointer_x: 150,
        pointer_y: 120,
        ..Default::default()
    });
    let kbd_pos = app.session.as_ref().unwrap().keyboard_pointer.unwrap();
    assert_eq!(
        app.session.as_ref().unwrap().active_pointer(150, 120),
        (kbd_pos.0.round() as i32, kbd_pos.1.round() as i32)
    );

    // 2. Physical mouse motion without clicking clears keyboard_pointer so Mickey hand tracks (150, 120).
    app.handle_input(&AppInput {
        pointer_x: 150,
        pointer_y: 120,
        pointer_moved: true,
        ..Default::default()
    });
    assert!(app.session.as_ref().unwrap().keyboard_pointer.is_none());
    assert_eq!(
        app.session.as_ref().unwrap().active_pointer(150, 120),
        (150, 120)
    );

    let mut renderer = AppRenderer::new();
    assert!(renderer.render_if_needed(&app, 150, 120, false, false));
    // Playing idle uses sprite 64 (open hand) with hotspot (-7, 0).
    let off_150 = (120 * 320 + (150 - 7)) * 4;
    assert_eq!(
        &renderer.rgba_buf[off_150..off_150 + 4],
        &[255, 250, 240, 255]
    );
}

#[test]
fn quit_phase_waits_for_active_sfx_with_bounded_fallback() {
    let mut app = synthetic_app();
    app.set_screen(AppScreen::Browser);

    // Without active audio sinks, Quit exits immediately.
    app.request_quit();
    assert!(app.quit_requested);
    assert!(app.should_exit());

    // With an active Rodio sink, Quit enters a noninteractive wait until playback ends or times out.
    app.quit_requested = false;
    app.quit_elapsed = 0.0;
    let (sink, _queue) = rodio::Sink::new_idle();
    sink.append(rodio::buffer::SamplesBuffer::new(
        1,
        11_025,
        vec![100i16; 4096],
    ));
    app.audio.sfx_sinks.push(sink);

    app.request_quit();
    assert!(app.quit_requested);
    assert!(!app.should_exit());

    // Interactive keys during quitting phase are ignored, while quit_elapsed advances.
    app.handle_input(&AppInput {
        dt: 0.5,
        key_p: true,
        ..Default::default()
    });
    assert!(app.browser_status.is_none());
    assert!(!app.should_exit());

    // Bounded fallback exits cleanly once elapsed >= 3.0s even if the sink hasn't drained.
    app.handle_input(&AppInput {
        dt: 2.6,
        ..Default::default()
    });
    assert!(app.should_exit());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_renders_original_tbx_sprites_in_vga_and_ega() {
    for ega in [false, true] {
        let mut app = test_browser();
        app.setup_menu.prefs.video_mode = if ega { 2 } else { 1 };
        app.sync_prefs_from_setup();
        let mut renderer = AppRenderer::new();
        app.browser_hover = None;
        app.browser_pressed = None;
        renderer.render(&app, 160, 100);
        let fb = &renderer.fb;

        // Left sidebar (0..32) and right sidebar (304..320) have non-uniform original sprite pixels.
        let left_colors: std::collections::HashSet<u8> =
            (10..50).map(|y| fb.get_pixel(16, y)).collect();
        assert!(left_colors.len() > 1, "left sidebar empty for ega={ega}");

        let right_colors: std::collections::HashSet<u8> =
            (4..30).map(|y| fb.get_pixel(312, y)).collect();
        assert!(
            right_colors.len() > 1,
            "right MORE header empty for ega={ega}"
        );

        // Print (36, 160) and OK (199, 160) button frames render multi-color TBX sprites.
        let print_colors: std::collections::HashSet<u8> =
            (165..185).map(|y| fb.get_pixel(60, y)).collect();
        assert!(
            print_colors.len() > 1,
            "Print TBX sprite empty for ega={ega}"
        );

        let ok_colors: std::collections::HashSet<u8> =
            (212..234).map(|x| fb.get_pixel(x, 175)).collect();
        assert!(ok_colors.len() > 1, "OK TBX sprite empty for ega={ega}");

        // Verify the 270px preview interior starts at x=34 (native x=2) and ends at x=303 (native x=271)
        // with zero horizontal shift or right-edge cropping.
        let mut centered = Framebuffer::new();
        let mut native = Framebuffer::new();
        crate::graphics::render_scene_view_with_scratch(
            &mut centered,
            &mut native,
            &app.preview_asset,
            app.preview_asset.static_frame_idx(),
        );
        for y in 20..159 {
            for x in 34..304 {
                assert_eq!(
                    fb.get_pixel(x, y),
                    native.get_pixel(x - 32, y),
                    "preview pixel mismatch at ({x}, {y}) for ega={ega}"
                );
            }
        }
    }
}

#[test]
fn browser_preview_preserves_left_and_right_edge_markers_without_shift() {
    use crate::assets::NamedSequence;

    let mut app = synthetic_app();
    app.set_screen(AppScreen::Browser);
    while app.preview_asset.sequences.len() < 2 {
        app.preview_asset.sequences.push(NamedSequence {
            name: "STILL".to_string(),
            header_idx: 0,
            start_frame: 0,
            end_frame: 0,
        });
    }
    app.preview_asset.sequences[1].start_frame = 0;
    app.preview_asset.palette[3] = [255, 0, 0];
    app.preview_asset.palette[4] = [0, 255, 0];
    let spr = &mut app.preview_asset.sprites[0];
    for y in 0..200 {
        let row = y * (spr.stride as usize);
        for x in 2..6 {
            spr.pixels[row + x] = 3;
        }
        for x in 268..272 {
            spr.pixels[row + x] = 4;
        }
    }

    let mut renderer = AppRenderer::new();
    renderer.render(&app, 160, 100);
    let fb = &renderer.fb;

    // Left edge marker (native x=2..5) lands at Browser x=34..37.
    for x in 34..38 {
        assert_eq!(fb.get_pixel(x, 80), 3, "left edge missing at x={x}");
    }
    // Right edge marker (native x=268..271) lands at Browser x=300..303.
    for x in 300..304 {
        assert_eq!(fb.get_pixel(x, 80), 4, "right edge missing at x={x}");
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_hover_pressed_and_right_sidebar_slot_selection() {
    let mut app = test_browser();
    let mut renderer = AppRenderer::new();

    // Initial render
    assert!(renderer.render_if_needed(&app, 160, 80, false, false));

    // Hovering over NextPuzzle changes browser_hover and triggers a re-render.
    let next_btn = BROWSER_BUTTONS[1];
    assert_eq!(next_btn.action, crate::app::BrowserAction::NextPuzzle);
    app.handle_input(&AppInput {
        pointer_x: next_btn.x + next_btn.w / 2,
        pointer_y: next_btn.y + next_btn.h / 2,
        pointer_moved: true,
        ..Default::default()
    });
    assert_eq!(
        app.browser_hover,
        Some(crate::app::BrowserTarget::Button(
            crate::app::BrowserAction::NextPuzzle
        ))
    );
    assert!(renderer.render_if_needed(
        &app,
        next_btn.x + next_btn.w / 2,
        next_btn.y + next_btn.h / 2,
        false,
        false
    ));

    // Clicking right-sidebar slot 5 selects puzzle 5 and sets browser_pressed until release.
    let slot5_y = crate::app::BROWSER_SLOT_Y0 + 4 * crate::app::BROWSER_SLOT_H + 5;
    app.handle_input(&AppInput {
        pointer_x: 312,
        pointer_y: slot5_y,
        left_pressed: true,
        ..Default::default()
    });
    assert_eq!(app.selected_puzzle, 5);
    assert_eq!(
        app.browser_pressed,
        Some(crate::app::BrowserTarget::PuzzleSlot(5))
    );
    app.handle_input(&AppInput {
        pointer_x: 312,
        pointer_y: slot5_y,
        left_released: true,
        ..Default::default()
    });
    assert!(app.browser_pressed.is_none());
}

#[test]
fn mickey_cursor_uses_correct_sprites_hotspots_and_viewport_bounds() {
    let mut app = synthetic_app();
    let mut palette = [[0u8; 3]; 256];
    palette[62] = [255, 0, 0];
    palette[63] = [0, 255, 0];
    palette[64] = [0, 0, 255];
    app.tbx.palette = palette;
    for idx in 0..=64u8 {
        let color = if idx >= 62 { idx } else { 1 };
        app.tbx.sprites.push(crate::assets::Sprite8061::from_8bpp(
            1,
            1,
            0,
            vec![color],
            vec![true],
        ));
    }

    let mut renderer = AppRenderer::new();

    // Browser idle -> sprite 62 (red), hotspot (-6, 0).
    app.set_screen(AppScreen::Browser);
    renderer.render_if_needed(&app, 100, 80, false, false);
    let off_62 = (80 * 320 + (100 - 6)) * 4;
    assert_eq!(&renderer.rgba_buf[off_62..off_62 + 4], &[255, 0, 0, 255]);

    // Browser pressed -> sprite 63 (green), hotspot (-10, 0).
    app.browser_pressed = Some(crate::app::BrowserTarget::PuzzleSlot(1));
    renderer.render_if_needed(&app, 100, 80, false, false);
    let off_63 = (80 * 320 + (100 - 10)) * 4;
    assert_eq!(&renderer.rgba_buf[off_63..off_63 + 4], &[0, 255, 0, 255]);
    app.browser_pressed = None;

    // Playing idle -> sprite 64 (blue), hotspot (-7, 0).
    app.session = Some(PuzzleSession::new(
        1,
        app.preview_asset.clone(),
        2,
        false,
        false,
        false,
    ));
    app.set_screen(AppScreen::Playing);
    assert_eq!(app.screen, AppScreen::Playing);
    renderer.render_if_needed(&app, 100, 80, false, false);
    let off_64 = (80 * 320 + (100 - 7)) * 4;
    assert_eq!(&renderer.rgba_buf[off_64..off_64 + 4], &[0, 0, 255, 255]);

    // Playing over open toolbox (px < 46) -> sprite 62 (red), hotspot (-6, 0).
    app.session.as_mut().unwrap().toolbox_open = true;
    renderer.render_if_needed(&app, 20, 80, false, false);
    let off_62_tbx = (80 * 320 + (20 - 6)) * 4;
    assert_eq!(
        &renderer.rgba_buf[off_62_tbx..off_62_tbx + 4],
        &[255, 0, 0, 255]
    );
    app.session.as_mut().unwrap().toolbox_open = false;

    // Out-of-viewport pointer -> cursor is not drawn and stays cached as None.
    assert!(renderer.render_if_needed(&app, -1, 80, false, false));
    assert!(!renderer.render_if_needed(&app, 320, 80, false, false));
    assert_ne!(&renderer.rgba_buf[off_64..off_64 + 4], &[0, 0, 255, 255]);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn toolbox_open_closed_hover_pressed_and_cursor_states() {
    let mut app = test_browser();
    let asset = app.preview_asset.clone();
    let mut session = PuzzleSession::new(1, asset, 3, true, false, false);
    session.toolbox_open = true;
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    let mut renderer = AppRenderer::new();

    // Open toolbox: x=0..2 stays black, tbx[49] icon renders inside Menu button at (3, 2).
    renderer.render_if_needed(&app, 200, 100, false, false);
    let black_idx = renderer.fb.color_index([0, 0, 0]);
    assert_eq!(renderer.fb.get_pixel(1, 100), black_idx);
    assert_eq!(
        AppRenderer::desired_cursor(&app, 200, 100, false),
        Some((64, 193, 100)),
        "playfield idle uses cursor 64"
    );
    let menu_baseline = renderer.fb.pixels;

    let icon_49 = &app.tbx.sprites[49];
    let mut checked_icon_px = false;
    for dy in 0..icon_49.sub_h as i32 {
        for dx in 0..icon_49.width as i32 {
            let sy = icon_49.sub_y as usize + dy as usize;
            let off = sy * icon_49.stride as usize + icon_49.sub_x as usize + dx as usize;
            if icon_49.is_opaque(off) {
                let expected = renderer
                    .fb
                    .color_index(app.tbx.palette[icon_49.pixels[off] as usize]);
                assert_eq!(renderer.fb.get_pixel(3 + dx, 2 + dy), expected);
                checked_icon_px = true;
                break;
            }
        }
        if checked_icon_px {
            break;
        }
    }
    assert!(checked_icon_px, "tbx[49] should have opaque pixels");

    // Hovering Menu, ScrollUp (inactive at 0), ScrollDown, and TraySlot(0).
    renderer.render_if_needed(&app, 20, 8, false, false);
    assert_ne!(renderer.fb.pixels, menu_baseline, "Menu hover must repaint");
    assert_eq!(
        AppRenderer::desired_cursor(&app, 20, 8, false),
        Some((62, 14, 8))
    );

    renderer.render_if_needed(&app, 20, 192, false, false);
    assert_ne!(
        renderer.fb.pixels, menu_baseline,
        "ScrollDown hover must repaint"
    );
    let hover_scroll_down = renderer.fb.pixels;

    // Pressing ScrollDown switches visual to pressed state and cursor to 63, then scrolls on release.
    {
        let sess = app.session.as_mut().unwrap();
        assert_eq!(
            sess.toolbox_target_at(20, 192),
            Some(crate::puzzle::ToolboxTarget::ScrollDown)
        );
        sess.handle_pointer_press(20, 192, &mut app.audio);
        assert_eq!(
            sess.pressed_toolbox_target,
            Some(crate::puzzle::ToolboxTarget::ScrollDown)
        );
    }
    renderer.render_if_needed(&app, 20, 192, false, false);
    assert_ne!(
        renderer.fb.pixels, hover_scroll_down,
        "ScrollDown press must repaint"
    );
    assert_eq!(
        AppRenderer::desired_cursor(&app, 20, 192, false),
        Some((63, 10, 192))
    );

    {
        let sess = app.session.as_mut().unwrap();
        sess.handle_pointer_release(20, 192, &mut app.audio);
        assert!(sess.pressed_toolbox_target.is_none());
        assert_eq!(sess.tray_scroll, 1);
    }

    // Collapsed toolbox: tab at (2, 2) renders tbx[49], responds to hover/press/release.
    app.session.as_mut().unwrap().toggle_toolbox();
    assert!(!app.session.as_ref().unwrap().toolbox_open);
    renderer.render_if_needed(&app, 200, 100, false, false);
    let closed_idle = renderer.fb.pixels;
    assert_eq!(
        app.session.as_ref().unwrap().toolbox_target_at(8, 8),
        Some(crate::puzzle::ToolboxTarget::ClosedTab)
    );
    assert_eq!(app.session.as_ref().unwrap().toolbox_target_at(20, 8), None);
    assert_eq!(
        AppRenderer::desired_cursor(&app, 8, 8, false),
        Some((62, 2, 8))
    );

    renderer.render_if_needed(&app, 8, 8, false, false);
    assert_ne!(
        renderer.fb.pixels, closed_idle,
        "ClosedTab hover must repaint"
    );

    app.session
        .as_mut()
        .unwrap()
        .handle_pointer_press(8, 8, &mut app.audio);
    assert_eq!(
        AppRenderer::desired_cursor(&app, 8, 8, false),
        Some((63, -2, 8))
    );
    renderer.render_if_needed(&app, 8, 8, false, false);
    app.session
        .as_mut()
        .unwrap()
        .handle_pointer_release(8, 8, &mut app.audio);
    assert!(app.session.as_ref().unwrap().toolbox_open);

    // ClosedTab horizontal edges (x = 0, 16, 17 open; x = 18 does not) and while holding a piece.
    for edge_x in [0, 16, 17] {
        let sess = app.session.as_mut().unwrap();
        sess.toolbox_open = false;
        assert_eq!(
            sess.toolbox_target_at(edge_x, 8),
            Some(crate::puzzle::ToolboxTarget::ClosedTab)
        );
        sess.handle_pointer_press(edge_x, 8, &mut app.audio);
        sess.handle_pointer_release(edge_x, 8, &mut app.audio);
        assert!(
            sess.toolbox_open,
            "ClosedTab edge x={edge_x} must open toolbox"
        );
    }
    {
        let sess = app.session.as_mut().unwrap();
        sess.toolbox_open = false;
        assert_eq!(sess.toolbox_target_at(18, 8), None);
        sess.handle_pointer_press(18, 8, &mut app.audio);
        sess.handle_pointer_release(18, 8, &mut app.audio);
        assert!(!sess.toolbox_open, "x=18 is outside ClosedTab");

        // Holding a piece while clicking rightmost tbx[49] column (x=17) still opens toolbox.
        sess.held_piece = Some(0);
        sess.handle_pointer_press(17, 8, &mut app.audio);
        sess.handle_pointer_release(17, 8, &mut app.audio);
        assert!(sess.toolbox_open);
        assert_eq!(sess.held_piece, Some(0));
    }
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn toolbox_tray_slots_scrolling_removal_and_return() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut session = PuzzleSession::new(1, asset, 3, true, false, false);
    let vis = session.visible_tray_slots();
    let slot_h = session.tray_slot_height();
    assert_eq!(vis, 3);

    // Every visible slot (first through last) is hit-tested accurately; non-slot regions return None.
    assert_eq!(
        session.toolbox_target_at(20, 8),
        Some(crate::puzzle::ToolboxTarget::Menu)
    );
    assert_eq!(
        session.toolbox_target_at(20, 22),
        Some(crate::puzzle::ToolboxTarget::ScrollUp)
    );
    assert_eq!(
        session.toolbox_target_at(20, 192),
        Some(crate::puzzle::ToolboxTarget::ScrollDown)
    );
    assert_eq!(session.hovered_tray_slot(20, 8), None);
    assert_eq!(session.hovered_tray_slot(20, 22), None);
    assert_eq!(session.hovered_tray_slot(20, 192), None);
    assert_eq!(session.hovered_tray_slot(48, 50), None);
    for slot in 0..vis {
        let y = 30 + slot as i32 * slot_h + 2;
        assert_eq!(session.hovered_tray_slot(20, y), Some(slot));
    }

    // Scroll boundaries, non-interrupting UI click audio, & tray removal + return.
    audio.play_common(crate::audio::CommonSound::ChoosePiece);
    let caption_before = audio.caption_key();
    assert!(!session.scroll_tray(-1));
    assert!(session.scroll_tray(1));
    assert_eq!(session.tray_scroll, 1);
    session.handle_click(20, 192, &mut audio);
    assert_eq!(session.tray_scroll, 2);
    assert_eq!(
        audio.caption_key(),
        caption_before,
        "tray scroll UI click must not clear active speech caption"
    );
    session.handle_click(20, 22, &mut audio);
    assert_eq!(session.tray_scroll, 1);

    let expected_first_after_scroll = session.tray_order[1];
    session.handle_click(20, 32, &mut audio);
    assert_eq!(session.held_piece, Some(expected_first_after_scroll));
    assert_eq!(session.tray_order.len(), 8);

    // Returning piece to slot 0 restores it at tray_scroll.
    session.handle_click(20, 32, &mut audio);
    assert!(session.held_piece.is_none());
    assert_eq!(session.tray_order.len(), 9);
    assert_eq!(session.tray_order[1], expected_first_after_scroll);

    // Picking pieces from the last page until tray_scroll >= tray_order.len() clamps scroll so tray is not empty.
    session.tray_scroll = session.tray_order.len() - vis;
    for _ in 0..vis {
        let y = 30 + (vis as i32 - 1) * slot_h + 2;
        session.handle_click(20, y, &mut audio);
        if let Some(p) = session.held_piece.take() {
            session.pieces[p].placed = true;
        }
    }
    assert!(session.tray_scroll < session.tray_order.len());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn connected_group_pickup_render_movement_and_tray_return() {
    let mut app = test_browser();
    let asset = app.preview_asset.clone();
    let mut session = PuzzleSession::new(1, asset, 2, true, false, false);
    let col_w = crate::puzzle::grid_col_widths(2);
    let row_h = crate::puzzle::grid_row_heights(2);
    let p0 = 0usize;
    let p1 = 1usize;
    for (pidx, r, c) in [(p0, 0usize, 0usize), (p1, 0, 1)] {
        let pos = session.tray_order.iter().position(|&p| p == pidx).unwrap();
        session.tray_scroll = pos;
        session.handle_click(20, 40, &mut app.audio);
        let cx = session.playfield_x() + col_w[..c].iter().sum::<i32>() + col_w[c] / 2;
        let cy = session.playfield_y() + row_h[..r].iter().sum::<i32>() + row_h[r] / 2;
        session.handle_click(cx, cy, &mut app.audio);
    }

    let c0x = session.playfield_x() + col_w[0] / 2;
    let c0y = session.playfield_y() + row_h[0] / 2;
    assert!(session.handle_right_click(c0x, c0y, &mut app.audio));
    assert_eq!(session.held_piece, Some(p0));
    assert_eq!(session.held_group.len(), 2);

    app.session = Some(session);
    app.set_screen(AppScreen::Playing);
    let mut renderer = AppRenderer::new();

    renderer.render_if_needed(&app, 150, 90, false, false);
    let frame_pos1 = renderer.fb.pixels;
    renderer.render_if_needed(&app, 200, 110, false, false);
    assert_ne!(
        renderer.fb.pixels, frame_pos1,
        "moving held connected group must update rendered playfield"
    );

    // Return the entire connected group to the tray.
    let sess = app.session.as_mut().unwrap();
    sess.handle_click(20, 40, &mut app.audio);
    assert!(sess.held_piece.is_none());
    assert!(sess.held_group.is_empty());
    assert!(sess.tray_order.contains(&p0));
    assert!(sess.tray_order.contains(&p1));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn vga_ega_switch_during_unfinished_puzzle_remaps_toolbox_and_border() {
    let mut app = test_browser();
    app.setup_menu.prefs.video_mode = 1; // VGA
    app.sync_prefs_from_setup();
    let mut session = PuzzleSession::new(1, app.preview_asset.clone(), 2, true, false, false);
    let pos = session.tray_order.iter().position(|&p| p == 0).unwrap();
    session.tray_scroll = pos;
    session.handle_click(20, 40, &mut app.audio);
    let piece0 = &session.pieces[0];
    let cx = session.playfield_x() + piece0.base_x + piece0.base_w / 2;
    let cy = session.playfield_y() + piece0.base_y + piece0.base_h / 2;
    session.handle_click(cx, cy, &mut app.audio);
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    let mut renderer = AppRenderer::new();
    renderer.render_if_needed(&app, 200, 100, false, false);
    let vga_pixels = renderer.fb.pixels;
    let vga_asset_rev = app.session.as_ref().unwrap().asset_rev();

    // Switch to EGA while puzzle is unfinished.
    app.setup_menu.prefs.video_mode = 2;
    app.sync_prefs_from_setup();
    assert!(app.error.is_none());
    let sess = app.session.as_ref().unwrap();
    assert_ne!(sess.asset_rev(), vga_asset_rev);
    assert!(sess.pieces[0].placed);

    renderer.render_if_needed(&app, 200, 100, false, false);
    assert_ne!(
        renderer.fb.pixels, vga_pixels,
        "EGA switch must invalidate toolbox & board caches"
    );

    // Verify tbx_ega[49] in open Menu is remapped to the EGA puzzle palette.
    let icon_49 = &app.tbx_ega.sprites[49];
    for dy in 0..icon_49.sub_h as i32 {
        for dx in 0..icon_49.width as i32 {
            let sy = icon_49.sub_y as usize + dy as usize;
            let off = sy * icon_49.stride as usize + icon_49.sub_x as usize + dx as usize;
            if icon_49.is_opaque(off) {
                let expected = renderer
                    .fb
                    .color_index(app.tbx_ega.palette[icon_49.pixels[off] as usize]);
                assert_eq!(renderer.fb.get_pixel(3 + dx, 2 + dy), expected);
                return;
            }
        }
    }
    panic!("tbx[49] had no opaque pixel");
}

#[test]
fn in_game_menu_opens_closes_and_navigates_keyboard_and_mouse() {
    let mut app = synthetic_app();
    while app.tbx.sprites.len() <= 64 {
        app.tbx.sprites.push(crate::assets::Sprite8061::from_8bpp(
            16,
            16,
            0,
            vec![1; 256],
            vec![true; 256],
        ));
    }
    let session = PuzzleSession::new(1, app.preview_asset.clone(), 2, true, false, true);
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    // Click MENU button in open toolbox (20, 8) -> press + release opens menu.
    app.handle_input(&AppInput {
        pointer_x: 20,
        pointer_y: 8,
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.in_game_menu_open);
    app.handle_input(&AppInput {
        pointer_x: 20,
        pointer_y: 8,
        left_released: true,
        ..Default::default()
    });
    assert!(app.in_game_menu_open);
    assert_eq!(app.in_game_menu_selected, 0);

    // Cursor is pointing hand (62) over menu; present_signature tracks hover/press.
    let mut renderer = AppRenderer::new();
    renderer.render_if_needed(&app, 120, 54, false, false);
    assert_eq!(
        AppRenderer::desired_cursor(&app, 120, 54, false),
        Some((62, 114, 54))
    );
    let sig_hover0 = app.present_signature(120, 54, false, false);
    let sig_hover1 = app.present_signature(120, 74, false, false);
    assert_ne!(sig_hover0, sig_hover1);

    // Down skips disabled Replay (when unsolved) and wraps cleanly.
    // Enabled items when unsolved: 0 OtherPicture, 1 Setup, 2 PieceTray, 4 Quit, 5 Resume.
    for expected in [1, 2, 4, 5, 0] {
        app.handle_input(&AppInput {
            down: true,
            ..Default::default()
        });
        assert_eq!(app.in_game_menu_selected, expected);
    }
    app.handle_input(&AppInput {
        up: true,
        ..Default::default()
    });
    assert_eq!(app.in_game_menu_selected, 5);

    // Press on Resume, drag off before release -> does NOT close menu.
    let (rx, ry, _, _) = crate::app::in_game_menu_row_rect(5);
    app.handle_input(&AppInput {
        pointer_x: rx + 10,
        pointer_y: ry + 5,
        left_pressed: true,
        ..Default::default()
    });
    assert_eq!(
        app.in_game_menu_pressed,
        Some(crate::app::InGameMenuCommand::Resume)
    );
    assert_eq!(
        AppRenderer::desired_cursor(&app, rx + 10, ry + 5, false),
        Some((63, rx, ry + 5))
    );
    app.handle_input(&AppInput {
        pointer_x: rx + 10,
        pointer_y: ry + 40,
        left_released: true,
        ..Default::default()
    });
    assert!(app.in_game_menu_open);
    assert!(app.in_game_menu_pressed.is_none());

    // Clicking outside menu panel closes it cleanly.
    app.handle_input(&AppInput {
        pointer_x: 10,
        pointer_y: 10,
        left_pressed: true,
        ..Default::default()
    });
    assert!(!app.in_game_menu_open);

    // Re-open and close with Escape; keyboard-to-mouse switching clears keyboard pointer.
    app.open_in_game_menu();
    app.handle_input(&AppInput {
        dt: 0.1,
        move_x: 1,
        ..Default::default()
    });
    assert!(app.session.as_ref().unwrap().keyboard_pointer.is_some());
    app.handle_input(&AppInput {
        pointer_x: 120,
        pointer_y: 54,
        pointer_moved: true,
        ..Default::default()
    });
    assert!(app.session.as_ref().unwrap().keyboard_pointer.is_none());
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(!app.in_game_menu_open);

    // Stationary pointer over disabled Replay (row 3) or OtherPicture (row 0)
    // must not override keyboard Down + Enter selection of Setup (row 1).
    app.open_in_game_menu();
    let (r0x, r0y, _, _) = crate::app::in_game_menu_row_rect(0);
    let (r3x, r3y, _, _) = crate::app::in_game_menu_row_rect(3);
    app.handle_input(&AppInput {
        pointer_x: r3x + 10,
        pointer_y: r3y + 5,
        pointer_moved: true,
        ..Default::default()
    });
    assert_eq!(app.in_game_menu_selected, 0);
    app.handle_input(&AppInput {
        pointer_x: r0x + 10,
        pointer_y: r0y + 5,
        down: true,
        ..Default::default()
    });
    assert_eq!(app.in_game_menu_selected, 1);
    app.handle_input(&AppInput {
        pointer_x: r0x + 10,
        pointer_y: r0y + 5,
        enter: true,
        ..Default::default()
    });
    assert!(matches!(app.screen, AppScreen::Setup { .. }));
    assert!(!app.in_game_menu_open);
    app.exit_setup(true);

    // WASD then physical mouse click *without* pointer_moved clears keyboard pointer
    // and hits physical mouse coordinates instead of stale keyboard pointer.
    app.open_in_game_menu();
    app.handle_input(&AppInput {
        dt: 0.1,
        move_x: 1,
        ..Default::default()
    });
    assert!(app.session.as_ref().unwrap().keyboard_pointer.is_some());
    let (r1x, r1y, _, _) = crate::app::in_game_menu_row_rect(1);
    app.handle_input(&AppInput {
        pointer_x: r1x + 10,
        pointer_y: r1y + 5,
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.session.as_ref().unwrap().keyboard_pointer.is_none());
    app.handle_input(&AppInput {
        pointer_x: r1x + 10,
        pointer_y: r1y + 5,
        left_released: true,
        ..Default::default()
    });
    assert!(matches!(app.screen, AppScreen::Setup { .. }));
    app.exit_setup(true);

    // Menu UI clicks (open, Up/Down, empty-handed PieceTray) must not interrupt active captions/speech.
    app.audio.play_common(crate::audio::CommonSound::NotDoneYet);
    let active_caption = app.audio.common_caption();
    assert!(active_caption.is_some());
    app.open_in_game_menu();
    assert_eq!(app.audio.common_caption(), active_caption);
    app.handle_input(&AppInput {
        down: true,
        ..Default::default()
    });
    assert_eq!(app.audio.common_caption(), active_caption);
    app.execute_in_game_menu_command(crate::app::InGameMenuCommand::PieceTray);
    assert_eq!(app.audio.common_caption(), active_caption);
}

#[test]
fn in_game_menu_commands_other_picture_setup_tray_replay_and_quit() {
    let mut app = synthetic_app();
    let mut session = PuzzleSession::new(1, app.preview_asset.clone(), 2, true, false, true);
    // Pick up piece 0 from tray and place piece 1 on board to create unfinished state + held piece.
    session.held_piece = session.tray_order.pop();
    session.pieces[1].placed = true;
    session.elapsed_secs = 42.5;
    let held_id = session.held_piece;
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    // 1. Setup from menu preserves active puzzle, held piece, and timer.
    app.open_in_game_menu();
    let (sx, sy, _, _) = crate::app::in_game_menu_row_rect(1);
    app.handle_input(&AppInput {
        pointer_x: sx + 8,
        pointer_y: sy + 6,
        left_pressed: true,
        ..Default::default()
    });
    app.handle_input(&AppInput {
        pointer_x: sx + 8,
        pointer_y: sy + 6,
        left_released: true,
        ..Default::default()
    });
    assert_eq!(
        app.screen,
        AppScreen::Setup {
            return_to_playing: true
        }
    );
    app.exit_setup(true);
    assert_eq!(app.screen, AppScreen::Playing);
    assert!(!app.in_game_menu_open);
    {
        let sess = app.session.as_ref().unwrap();
        assert_eq!(sess.held_piece, held_id);
        assert!(sess.pieces[1].placed);
        assert!((sess.elapsed_secs - 42.5).abs() < 1e-4);
    }

    // 2. Replay is disabled while unsolved (both menu click and R hotkey).
    app.open_in_game_menu();
    let (rx, ry, _, _) = crate::app::in_game_menu_row_rect(3);
    app.handle_input(&AppInput {
        pointer_x: rx + 8,
        pointer_y: ry + 6,
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.in_game_menu_open);
    assert!(app.session.as_ref().unwrap().active_anim.is_none());
    app.close_in_game_menu(false);
    app.handle_input(&AppInput {
        key_r: true,
        ..Default::default()
    });
    assert!(app.session.as_ref().unwrap().active_anim.is_none());

    // 3. PieceTray returns held single piece when holding one, preserves held connected groups,
    //    and toggles tray when empty-handed.
    let tray_snapshot_before_group = {
        let sess = app.session.as_mut().unwrap();
        let extra = sess.tray_order.pop().unwrap();
        sess.held_group = vec![(extra, 10, 0)];
        (extra, sess.tray_order.clone())
    };
    app.execute_in_game_menu_command(crate::app::InGameMenuCommand::PieceTray);
    {
        let sess = app.session.as_mut().unwrap();
        assert_eq!(sess.held_piece, held_id);
        assert_eq!(sess.held_group, vec![(tray_snapshot_before_group.0, 10, 0)]);
        assert_eq!(sess.tray_order, tray_snapshot_before_group.1);
        // Restore single held piece for single-piece PieceTray test.
        let (extra, _, _) = sess.held_group.pop().unwrap();
        sess.tray_order.push(extra);
    }
    app.execute_in_game_menu_command(crate::app::InGameMenuCommand::PieceTray);
    {
        let sess = app.session.as_ref().unwrap();
        assert!(sess.held_piece.is_none());
        assert!(sess.toolbox_open);
        assert_eq!(sess.tray_order.first().copied(), held_id);
    }
    app.execute_in_game_menu_command(crate::app::InGameMenuCommand::PieceTray);
    assert!(!app.session.as_ref().unwrap().toolbox_open);
    app.execute_in_game_menu_command(crate::app::InGameMenuCommand::PieceTray);
    assert!(app.session.as_ref().unwrap().toolbox_open);

    // 4. Other Picture prompts unfinished confirmation; cancel preserves state, confirm returns to Browser.
    app.execute_in_game_menu_command(crate::app::InGameMenuCommand::OtherPicture);
    assert!(app.confirm_exit_puzzle);
    assert_eq!(app.audio.common_caption(), Some("We're not done yet."));
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(!app.confirm_exit_puzzle);
    assert_eq!(app.screen, AppScreen::Playing);
    assert!(app.session.as_ref().unwrap().pieces[1].placed);

    // 5. Quit from unfinished puzzle prompts confirmation, cancel preserves state, confirm quits.
    app.handle_input(&AppInput {
        key_q: true,
        ..Default::default()
    });
    assert!(app.confirm_quit_puzzle);
    assert!(!app.quit_requested);
    assert_eq!(app.audio.common_caption(), Some("We're not done yet."));
    app.handle_input(&AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(!app.confirm_quit_puzzle);
    assert!(!app.quit_requested);
    assert!(app.session.as_ref().unwrap().pieces[1].placed);

    app.execute_in_game_menu_command(crate::app::InGameMenuCommand::Quit);
    assert!(app.confirm_quit_puzzle);
    app.handle_input(&AppInput {
        enter: true,
        ..Default::default()
    });
    assert!(!app.confirm_quit_puzzle);
    assert!(app.quit_requested);
}

#[test]
fn in_game_solved_replay_and_quit_behavior() {
    let mut app = synthetic_app();
    let mut session = PuzzleSession::new(1, app.preview_asset.clone(), 2, true, false, true);
    session.auto_solve(&mut app.audio);
    session.active_anim = None;
    app.session = Some(session);
    app.set_screen(AppScreen::Playing);

    // PieceTray is disabled when solved; Replay is enabled.
    assert!(!app.is_in_game_menu_command_enabled(crate::app::InGameMenuCommand::PieceTray));
    assert!(app.is_in_game_menu_command_enabled(crate::app::InGameMenuCommand::Replay));

    // Clicking top-left menu hotspot when solved opens the command menu.
    app.handle_input(&AppInput {
        pointer_x: 5,
        pointer_y: 5,
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.in_game_menu_open);

    // Replay starts animation and plays SeeWhatHappens ("Let's see what happens.").
    app.execute_in_game_menu_command(crate::app::InGameMenuCommand::Replay);
    assert!(!app.in_game_menu_open);
    assert!(app.session.as_ref().unwrap().active_anim.is_some());
    assert_eq!(app.audio.common_caption(), Some("Let's see what happens."));

    // Repeated R hotkey restarts cleanly.
    app.handle_input(&AppInput {
        key_r: true,
        ..Default::default()
    });
    assert!(app.session.as_ref().unwrap().active_anim.is_some());

    // Quit when solved asks confirmation WITHOUT playing "We're not done yet."
    app.audio.stop_all();
    app.handle_input(&AppInput {
        key_q: true,
        ..Default::default()
    });
    assert!(app.confirm_quit_puzzle);
    assert_eq!(app.audio.common_caption(), None);
    app.handle_input(&AppInput {
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.quit_requested);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn in_game_menu_renders_original_tbx_sprites_in_vga_and_ega() {
    let mut app = GameApp::new(test_game_dir(), false).unwrap();
    app.selected_puzzle = 1;
    app.selected_grid_dim = 2;
    app.start_selected_puzzle();

    let mut renderer = AppRenderer::new();
    renderer.render_if_needed(&app, 200, 100, false, false);
    let board_only = renderer.fb.pixels;

    // Idle board pointer motion keeps base cache clean while menu is closed.
    assert_eq!(
        app.present_signature(200, 100, false, false),
        app.present_signature(201, 101, false, false)
    );
    assert!(!renderer.render_if_needed(&app, 200, 100, false, false));

    // Open menu in VGA and verify it renders over the board.
    app.open_in_game_menu();
    assert!(renderer.render_if_needed(&app, 120, 54, false, false));
    assert_ne!(renderer.fb.pixels, board_only);

    // Switch to EGA while menu is open -> Setup closes menu cleanly, then re-open in EGA.
    app.open_setup(true);
    assert!(!app.in_game_menu_open);
    app.setup_menu.prefs.video_mode = 2;
    app.exit_setup(true);
    app.open_in_game_menu();
    assert!(renderer.render_if_needed(&app, 120, 54, false, false));
}

fn composite_gpu_layers_sw(
    app: &GameApp,
    app_renderer: &mut AppRenderer,
    mx: i32,
    my: i32,
    picture_guide: bool,
) -> Vec<u8> {
    use crate::graphics::SCREEN_H;

    let sess = app.session.as_ref().unwrap();
    let tbx = if sess.asset.uses_ega() {
        &app.tbx_ega
    } else {
        &app.tbx
    };
    let (px, py) = sess.active_pointer(mx, my);
    let mut out = vec![0u8; (SCREEN_W * SCREEN_H * 4) as usize];

    let blit_layer = |dst: &mut [u8], src: &[u8], lx: i32, ly: i32, lw: i32, lh: i32| {
        for sy in 0..lh {
            let dy = ly + sy;
            if !(0..SCREEN_H).contains(&dy) {
                continue;
            }
            for sx in 0..lw {
                let dx = lx + sx;
                if !(0..SCREEN_W).contains(&dx) {
                    continue;
                }
                let s = ((sy * lw + sx) * 4) as usize;
                if src[s + 3] == 0 {
                    continue;
                }
                let d = ((dy * SCREEN_W + dx) * 4) as usize;
                dst[d..d + 4].copy_from_slice(&src[s..s + 4]);
            }
        }
    };

    app_renderer
        .puzzle_renderer
        .render_board_layer_rgba(sess, tbx, picture_guide, &mut out);

    if let Some((_, rem)) = sess.hint_piece {
        let mut hint_buf = Vec::new();
        if let Some((hx, hy, hw, hh)) = app_renderer
            .puzzle_renderer
            .render_hint_layer_rgba(sess, &mut hint_buf)
        {
            let blink = ((rem * 8.0) as i32) & 1 == 0;
            if !blink {
                for px in hint_buf.as_chunks_mut::<4>().0 {
                    if px[3] > 0 {
                        px[1] = 0;
                        px[2] = 0;
                    }
                }
            }
            blit_layer(&mut out, &hint_buf, hx, hy, hw, hh);
        }
    }

    let mut tbx_buf = Vec::new();
    let (tx, ty, tw, th) = app_renderer.puzzle_renderer.render_toolbox_layer_rgba(
        sess,
        tbx,
        &app.font,
        px,
        py,
        &mut tbx_buf,
    );
    blit_layer(&mut out, &tbx_buf, tx, ty, tw, th);

    if let Some(pidx) = sess.held_piece {
        let mut held_buf = Vec::new();
        if let Some((rx, ry, hw, hh)) =
            app_renderer
                .puzzle_renderer
                .render_held_group_layer_rgba(sess, px, py, &mut held_buf)
        {
            let anchor = &sess.pieces[pidx];
            let ax = px - anchor.base_w / 2;
            let ay = py - anchor.base_h / 2;
            blit_layer(&mut out, &held_buf, ax + rx, ay + ry, hw, hh);
        }
    }

    let mut overlay_buf = vec![0u8; (SCREEN_W * SCREEN_H * 4) as usize];
    if app_renderer.render_playing_overlay_rgba(app, &mut overlay_buf) {
        blit_layer(&mut out, &overlay_buf, 0, 0, SCREEN_W, SCREEN_H);
    }

    if let Some((sprite_idx, cx, cy)) = AppRenderer::desired_cursor(app, mx, my, false) {
        let mut cur_buf = Vec::new();
        if let Some((cw, ch)) = AppRenderer::render_cursor_rgba(app, sprite_idx, &mut cur_buf) {
            blit_layer(&mut out, &cur_buf, cx, cy, cw, ch);
        }
    }

    out
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn gpu_layer_composition_matches_software_renderer_pixel_for_pixel() {
    for video_mode in [0u16, 2u16] {
        let mut app = GameApp::new(test_game_dir(), false).unwrap();
        app.setup_menu.prefs.video_mode = video_mode;
        app.exit_setup(false);
        app.selected_puzzle = 1;
        app.selected_grid_dim = 4;
        app.start_selected_puzzle();

        let mut ref_renderer = AppRenderer::new();
        let mut layer_renderer = AppRenderer::new();

        // 1. Place two adjacent pieces (0 and 1), then pick up the connected group
        {
            let col_w = crate::puzzle::grid_col_widths(4);
            let row_h = crate::puzzle::grid_row_heights(4);
            let sess = app.session.as_mut().unwrap();
            for (pidx, r, c) in [(0usize, 0usize, 0usize), (1usize, 0, 1)] {
                let pos = sess.tray_order.iter().position(|&p| p == pidx).unwrap();
                sess.tray_scroll = pos;
                sess.handle_click(20, 40, &mut app.audio);
                let cx = sess.playfield_x() + col_w[..c].iter().sum::<i32>() + col_w[c] / 2;
                let cy = sess.playfield_y() + row_h[..r].iter().sum::<i32>() + row_h[r] / 2;
                sess.handle_click(cx, cy, &mut app.audio);
            }
            sess.trigger_hint();
            sess.status_message = Some(("Connected!".to_string(), 1.5));
        }

        for guide in [false, true] {
            for (mx, my) in [(160, 100), (24, 50)] {
                assert!(ref_renderer.render_if_needed(&app, mx, my, false, guide));
                let composited = composite_gpu_layers_sw(&app, &mut layer_renderer, mx, my, guide);
                if composited != ref_renderer.rgba_buf {
                    let diff_idx = composited
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(ref_renderer.rgba_buf.as_chunks::<4>().0.iter())
                        .position(|(a, b)| a != b)
                        .unwrap();
                    let dx = (diff_idx as i32) % SCREEN_W;
                    let dy = (diff_idx as i32) / SCREEN_W;
                    panic!(
                        "pixel mismatch at ({dx},{dy}) for video_mode={video_mode}, guide={guide}, pos=({mx},{my}): layer={:?} vs ref={:?}",
                        &composited[diff_idx * 4..diff_idx * 4 + 4],
                        &ref_renderer.rgba_buf[diff_idx * 4..diff_idx * 4 + 4]
                    );
                }
            }
        }

        // 2. Pick up connected group and drag it over both the board and toolbox
        {
            let sess = app.session.as_mut().unwrap();
            let pfx = sess.playfield_x();
            let pfy = sess.playfield_y();
            let tx = pfx + sess.pieces[0].base_x + sess.pieces[0].base_w / 2;
            let ty = pfy + sess.pieces[0].base_y + sess.pieces[0].base_h / 2;
            let mut audio = AudioEngine::new(&test_game_dir(), false, false);
            assert!(sess.handle_right_click(tx, ty, &mut audio));
        }
        for guide in [false, true] {
            for (mx, my) in [(150, 95), (38, 80)] {
                ref_renderer.render_if_needed(&app, mx, my, false, guide);
                let composited = composite_gpu_layers_sw(&app, &mut layer_renderer, mx, my, guide);
                if composited != ref_renderer.rgba_buf {
                    let diff_idx = composited
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(ref_renderer.rgba_buf.as_chunks::<4>().0.iter())
                        .position(|(a, b)| a != b)
                        .unwrap();
                    let dx = (diff_idx as i32) % SCREEN_W;
                    let dy = (diff_idx as i32) / SCREEN_W;
                    panic!(
                        "held-group mismatch at ({dx},{dy}) for video_mode={video_mode}, guide={guide}, pos=({mx},{my}): layer={:?} vs ref={:?}",
                        &composited[diff_idx * 4..diff_idx * 4 + 4],
                        &ref_renderer.rgba_buf[diff_idx * 4..diff_idx * 4 + 4]
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
fn touch_input_drives_puzzle_drag_placement_tray_and_menus() {
    use crate::input::{InputRouter, PlatformEvent, ViewportTransform};

    let mut app = GameApp::new(test_game_dir(), false).unwrap();
    app.selected_puzzle = 1;
    app.selected_grid_dim = 2;
    app.start_selected_puzzle();

    let tf = ViewportTransform::new(960.0, 720.0, 1.5, true);
    let mut router = InputRouter::new();
    let dos_to_logical = |dx: i32, dy: i32| -> (f32, f32) {
        (
            tf.viewport.x + (dx as f32 + 0.5) * tf.scale,
            tf.viewport.y + (dy as f32 + 0.5) * tf.scale,
        )
    };

    // 1. Touch drag on tray slot 0 picks up piece, drag to target places it
    let tray_pidx = app.session.as_ref().unwrap().tray_order[0];
    let (tx, ty) = dos_to_logical(23, 44);
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 1,
        x: tx,
        y: ty,
    });
    let f1 = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f1.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_piece, None);

    router.push_event(PlatformEvent::TouchMove {
        finger_id: 1,
        x: tx + 12.0,
        y: ty,
    });
    let f1b = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f1b.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_piece, Some(tray_pidx));

    let (dest_x, dest_y) = {
        let sess = app.session.as_ref().unwrap();
        let p = &sess.pieces[tray_pidx];
        dos_to_logical(
            sess.playfield_x() + p.base_x + p.base_w / 2,
            sess.playfield_y() + p.base_y + p.base_h / 2,
        )
    };
    router.push_event(PlatformEvent::TouchMove {
        finger_id: 1,
        x: dest_x,
        y: dest_y,
    });
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 1,
        x: dest_x,
        y: dest_y,
    });
    let f2 = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f2.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_piece, None);
    assert!(app.session.as_ref().unwrap().pieces[tray_pidx].placed);
    assert!(app.session.as_ref().unwrap().toolbox_open, "after step 1");

    // 2. Two-finger vertical drag on empty board area scrolls tray
    let initial_scroll = app.session.as_ref().unwrap().tray_scroll;
    let empty_x = dest_x + 400.0;
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 2,
        x: empty_x,
        y: dest_y,
    });
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 3,
        x: empty_x + 40.0,
        y: dest_y,
    });
    router.push_event(PlatformEvent::TouchMove {
        finger_id: 3,
        x: empty_x + 40.0,
        y: dest_y - 30.0,
    });
    let f_scroll = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_scroll.app_input);
    assert_eq!(
        app.session.as_ref().unwrap().tray_scroll,
        initial_scroll + 1
    );
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 2,
        x: empty_x,
        y: dest_y,
    });
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 3,
        x: empty_x + 40.0,
        y: dest_y - 30.0,
    });
    let _ = router.finish_frame(0.016, &tf, true, true);

    // 3. Long-press on placed piece picks it up as connected group (right-click alternative)
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 4,
        x: dest_x,
        y: dest_y,
    });
    let f_lp_down = router.finish_frame(0.1, &tf, true, true);
    app.handle_input(&f_lp_down.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_piece, None);
    let f_lp = router.finish_frame(0.4, &tf, true, true);
    app.handle_input(&f_lp.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_piece, Some(tray_pidx));
    assert!(!app.session.as_ref().unwrap().pieces[tray_pidx].placed);
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 4,
        x: dest_x,
        y: dest_y,
    });
    let f_lp_up = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_lp_up.app_input);
    // Piece remains held after long-press lift so user can tap destination or tray to return
    assert_eq!(app.session.as_ref().unwrap().held_piece, Some(tray_pidx));

    // 4. Tap tray slot returns held piece to tray
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 5,
        x: tx,
        y: ty,
    });
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 5,
        x: tx,
        y: ty,
    });
    let f_ret = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_ret.app_input);
    let f_ret_rel = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_ret_rel.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_piece, None);

    // 5. Tap MENU button opens modal menu
    let (mx, my) = dos_to_logical(22, 9);
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 6,
        x: mx,
        y: my,
    });
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 6,
        x: mx,
        y: my,
    });
    let f_menu = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_menu.app_input);
    let f_menu_rel = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_menu_rel.app_input);
    assert!(app.in_game_menu_open);

    // 6. Close modal menu, place a 2-piece edge-connected cluster, and verify both
    //    long-press and two-finger tap lift the entire 2-piece group (forwarding every frame).
    app.close_in_game_menu(false);
    {
        let sess = app.session.as_mut().unwrap();
        let mut audio = AudioEngine::new(&test_game_dir(), false, false);
        for pidx in [0usize, 1] {
            sess.tray_order.retain(|&id| id != pidx);
            sess.held_piece = Some(pidx);
            sess.held_group = vec![(pidx, 0, 0)];
            let px = sess.playfield_x() + sess.pieces[pidx].base_x + sess.pieces[pidx].base_w / 2;
            let py = sess.playfield_y() + sess.pieces[pidx].base_y + sess.pieces[pidx].base_h / 2;
            sess.handle_click(px, py, &mut audio);
            assert!(sess.held_piece.is_none());
            assert!(sess.pieces[pidx].placed);
        }
    }
    let (c_x, c_y) = {
        let sess = app.session.as_ref().unwrap();
        let p = &sess.pieces[0];
        dos_to_logical(
            sess.playfield_x() + p.base_x + p.base_w / 2,
            sess.playfield_y() + p.base_y + p.base_h / 2,
        )
    };
    // 6a. Long-press lifts both pieces
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 7,
        x: c_x,
        y: c_y,
    });
    let f_c_down = router.finish_frame(0.1, &tf, true, true);
    app.handle_input(&f_c_down.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_piece, None);
    let f_c_lp = router.finish_frame(0.4, &tf, true, true);
    app.handle_input(&f_c_lp.app_input);
    {
        let sess = app.session.as_ref().unwrap();
        assert_eq!(sess.held_piece, Some(0));
        assert_eq!(sess.held_group.len(), 2);
        assert!(!sess.pieces[0].placed && !sess.pieces[1].placed);
    }
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 7,
        x: c_x,
        y: c_y,
    });
    let f_c_up = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_c_up.app_input);

    // Place cluster back on board via tap at original position
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 8,
        x: c_x,
        y: c_y,
    });
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 8,
        x: c_x,
        y: c_y,
    });
    let f_place = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_place.app_input);
    let f_place_rel = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_place_rel.app_input);
    assert!(app.session.as_ref().unwrap().pieces[0].placed);
    assert!(app.session.as_ref().unwrap().pieces[1].placed);

    // 6b. Two-finger tap also lifts the entire 2-piece connected cluster
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 9,
        x: c_x,
        y: c_y,
    });
    let f_tf_down1 = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_tf_down1.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_piece, None);
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 10,
        x: c_x + 30.0,
        y: c_y,
    });
    let f_tf_down2 = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_tf_down2.app_input);
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 10,
        x: c_x + 30.0,
        y: c_y,
    });
    let f_tf_up2 = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_tf_up2.app_input);
    {
        let sess = app.session.as_ref().unwrap();
        assert_eq!(sess.held_piece, Some(0));
        assert_eq!(sess.held_group.len(), 2);
        assert!(!sess.pieces[0].placed && !sess.pieces[1].placed);
    }
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 9,
        x: c_x,
        y: c_y,
    });
    let f_tf_up1 = router.finish_frame(0.016, &tf, true, true);
    app.handle_input(&f_tf_up1.app_input);
    assert_eq!(app.session.as_ref().unwrap().held_group.len(), 2);
}
