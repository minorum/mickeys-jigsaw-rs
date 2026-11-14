use super::*;
use crate::app::{AppInput, AppRenderer, AppScreen, BROWSER_BUTTONS, GameApp, game_viewport};
use crate::assets::{
    FontAsset, Preferences, PrintAsset, PuzzleAsset, ToolboxAsset, load_ilbm_container,
};
use crate::audio::{AudioEngine, CommonSound, synthesize_iss_song, synthesize_nnv_phrase};
use crate::graphics::Framebuffer;
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
            let point = vec2(
                viewport.x + (x + 0.5) * scale,
                viewport.y + (y + 0.5) * scale,
            );
            assert_eq!(((point.x - viewport.x) / scale).floor() as i32, x as i32);
            assert_eq!(((point.y - viewport.y) / scale).floor() as i32, y as i32);
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
        assert_eq!(fb.get_pixel(7, 5), fb.color_index([0; 3]), "ega={ega}");
        assert_eq!(
            fb.get_pixel(7, 173),
            fb.color_index([0, 0, 170]),
            "ega={ega}"
        );
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
    let (_, ex, ew, _) = BROWSER_BUTTONS[3];
    app.handle_browser_click(ex + ew / 2, 180);
    assert_ne!(app.curved_edges, edges_was);

    let (_, sx, sw, _) = BROWSER_BUTTONS[4];
    app.handle_browser_click(sx + sw / 2, 180);
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

    eprintln!(
        "load_15_vga={:.2}ms load_15_ega={:.2}ms cut_8x8={:.2}ms cut_preset={:.2}ms \
         render_warm={:.2}us/frame drag_full={:.2}us/frame to_rgba8={:.2}us/frame \
         guide={:.2}us/frame render_anim={:.2}us/frame render_dissolve={:.2}us/frame",
        load_all_vga.as_secs_f64() * 1000.0,
        load_all_ega.as_secs_f64() * 1000.0,
        cut_8x8.as_secs_f64() * 1000.0,
        cut_preset.as_secs_f64() * 1000.0,
        render_warm.as_secs_f64() * 1e6 / iters as f64,
        drag_full.as_secs_f64() * 1e6 / iters as f64,
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
