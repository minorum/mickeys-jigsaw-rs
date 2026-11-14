use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use bevy_state::prelude::{NextState, StateTransition};

use super::*;
use crate::app::{
    AppInput, AppRenderer, AppScreen, BROWSER_BUTTONS, app_screen, build_game_app,
    build_game_app_from_parts, game_viewport, has_mickey_cursor, present_signature, should_exit,
    step_app,
};
use crate::assets::{
    FontAsset, Preferences, PrintAsset, PuzzleAsset, ToolboxAsset, load_ilbm_container,
};
use crate::audio::{AudioEngine, CommonSound, synthesize_iss_song, synthesize_nnv_phrase};
use crate::ecs::components::{
    AnimationState, HintHighlight, PieceGeometry, PieceIdentity, PieceLocation, PieceSprite,
    SceneHotspot,
};
use crate::ecs::plugins::sync_audio_status;
use crate::ecs::resources::{
    ActivePuzzle, AudioStatus, BrowserState, GameConfig, InGameMenuState, PuzzleMeta,
    RenderVersions, ScreenState, SetupState, StaticAssets,
};
use crate::ecs::systems::browser::{can_resume_selected, spawn_puzzle_session};
use crate::ecs::systems::setup::{replace_scene_asset_ecs, sync_prefs_from_setup};
use crate::graphics::{Framebuffer, SCREEN_W};
use crate::puzzle_render::PuzzleRenderer;
use crate::setup::SetupMenu;

fn app_meta(app: &App) -> &PuzzleMeta {
    app.world().resource::<ActivePuzzle>().0.as_ref().unwrap()
}

fn app_meta_mut(app: &mut App) -> &mut PuzzleMeta {
    app.world_mut()
        .resource_mut::<ActivePuzzle>()
        .into_inner()
        .0
        .as_mut()
        .unwrap()
}

fn app_held_piece(app: &mut App) -> Option<usize> {
    app.world_mut()
        .query::<(&PieceIdentity, &PieceLocation)>()
        .iter(app.world())
        .find_map(|(id, loc)| match *loc {
            PieceLocation::Held { is_anchor: true, .. } => Some(id.index),
            _ => None,
        })
}

fn app_tray_order(app: &mut App) -> Vec<usize> {
    let mut items: Vec<(usize, usize)> = app
        .world_mut()
        .query::<(&PieceIdentity, &PieceLocation)>()
        .iter(app.world())
        .filter_map(|(id, loc)| match *loc {
            PieceLocation::Tray { order } => Some((order, id.index)),
            _ => None,
        })
        .collect();
    items.sort_by_key(|&(ord, idx)| (ord, idx));
    items.into_iter().map(|(_, idx)| idx).collect()
}

fn app_is_placed(app: &mut App, idx: usize) -> bool {
    app.world_mut()
        .query::<(&PieceIdentity, &PieceLocation)>()
        .iter(app.world())
        .any(|(id, loc)| id.index == idx && loc.is_placed())
}

fn app_all_placed(app: &mut App) -> bool {
    app.world_mut()
        .query::<&PieceLocation>()
        .iter(app.world())
        .all(|loc| loc.is_placed())
}

fn app_piece_geom(app: &mut App, idx: usize) -> PieceGeometry {
    app.world_mut()
        .query::<(&PieceIdentity, &PieceGeometry)>()
        .iter(app.world())
        .find_map(|(id, g)| (id.index == idx).then_some(*g))
        .unwrap()
}

fn app_piece_count(app: &mut App) -> usize {
    app.world_mut()
        .query::<&PieceIdentity>()
        .iter(app.world())
        .count()
}

fn app_set_piece_placed(app: &mut App, idx: usize, placed: bool) {
    let mut target = None;
    let mut max_order = None::<usize>;
    for (e, id, loc) in app
        .world_mut()
        .query::<(Entity, &PieceIdentity, &PieceLocation)>()
        .iter(app.world())
    {
        if id.index == idx {
            target = Some((e, id.home_row, id.home_col));
        }
        if let PieceLocation::Tray { order } = *loc {
            max_order = Some(max_order.map_or(order, |m| m.max(order)));
        }
    }
    let (e, r, c) = target.unwrap();
    let mut loc = app.world_mut().get_mut::<PieceLocation>(e).unwrap();
    if placed {
        *loc = PieceLocation::Board { row: r, col: c };
    } else {
        *loc = PieceLocation::Tray {
            order: max_order.map_or(0, |m| m + 1),
        };
    }
    step_app(app, &AppInput::default());
}

fn app_set_piece_held(app: &mut App, idx: usize) {
    let e = app
        .world_mut()
        .query::<(Entity, &PieceIdentity)>()
        .iter(app.world())
        .find_map(|(e, id)| (id.index == idx).then_some(e))
        .unwrap();
    *app.world_mut().get_mut::<PieceLocation>(e).unwrap() = PieceLocation::Held {
        is_anchor: true,
        group_offset: (0, 0),
        group_order: 0,
    };
    step_app(app, &AppInput::default());
}

fn app_spawn_puzzle(
    app: &mut App,
    puzzle_num: usize,
    asset: PuzzleAsset,
    grid_dim: usize,
    curved_edges: bool,
    grid_lines: bool,
    timer_enabled: bool,
) {
    let text_enabled = app.world().resource::<GameConfig>().prefs.text_msg_on != 0;
    let mut slot = Some(asset);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut active_puzzle: ResMut<ActivePuzzle>,
                  piece_q: Query<Entity, With<PieceIdentity>>,
                  hotspot_q: Query<Entity, With<SceneHotspot>>,
                  anim_q: Query<Entity, With<AnimationState>>| {
                let asset = slot.take().unwrap();
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
                    text_enabled,
                );
            },
        )
        .unwrap();
    step_app(app, &AppInput::default());
}

fn app_replace_scene_asset(app: &mut App, asset: PuzzleAsset) {
    let mut slot = Some(asset);
    app.world_mut()
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
    step_app(app, &AppInput::default());
}

fn app_can_resume_selected(app: &App) -> bool {
    let browser = app.world().resource::<BrowserState>();
    let config = app.world().resource::<GameConfig>();
    let active = app.world().resource::<ActivePuzzle>();
    can_resume_selected(active, browser, config)
}

fn app_start_selected_puzzle(app: &mut App) {
    step_app(app, &AppInput {
        pointer_x: 210,
        pointer_y: 175,
        left_pressed: true,
        ..Default::default()
    });
    step_app(app, &AppInput {
        pointer_x: 210,
        pointer_y: 175,
        left_released: true,
        ..Default::default()
    });
}

fn app_restart_selected_puzzle(app: &mut App) {
    app.world_mut().resource_mut::<ActivePuzzle>().0 = None;
    app_set_screen(app, AppScreen::Browser);
    app_start_selected_puzzle(app);
}

fn app_right_click(app: &mut App, x: i32, y: i32) -> bool {
    step_app(app, &AppInput {
        pointer_x: x,
        pointer_y: y,
        right_pressed: true,
        ..Default::default()
    });
    app_held_piece(app).is_some()
}

fn app_set_screen(app: &mut App, screen: AppScreen) {
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(screen);
    let _ = app.world_mut().try_run_schedule(StateTransition);
    app.world_mut().resource_mut::<RenderVersions>().app_rev += 1;
}

fn app_active_anim(app: &mut App) -> Option<AnimationState> {
    let w = app.world_mut();
    w.query::<&AnimationState>().iter(w).next().cloned()
}

fn app_hint_piece(app: &mut App) -> Option<(usize, f32)> {
    let w = app.world_mut();
    w.query::<(&PieceIdentity, &HintHighlight)>()
        .iter(w)
        .next()
        .map(|(id, h)| (id.index, h.timer.remaining_secs()))
}

fn app_sync_prefs_from_setup(app: &mut App) {
    { let mut s = AudioStatus::default(); sync_audio_status(app.world().non_send::<AudioEngine>(), &mut s); *app.world_mut().resource_mut::<AudioStatus>() = s; }
    app.world_mut()
        .run_system_once(
            |mut commands: Commands,
             mut screen_state: ResMut<ScreenState>,
             mut setup: ResMut<SetupState>,
             mut config: ResMut<GameConfig>,
             mut browser: ResMut<BrowserState>,
             mut active_puzzle: ResMut<ActivePuzzle>,
             mut piece_q: Query<(Entity, &PieceIdentity, &mut PieceGeometry, &mut PieceSprite)>,
             hotspot_q: Query<Entity, With<SceneHotspot>>,
             anim_q: Query<Entity, With<AnimationState>>,
             mut audio_out: MessageWriter<crate::ecs::events::AudioCommand>| {
                sync_prefs_from_setup(
                    &mut commands,
                    &mut screen_state,
                    &mut setup,
                    &mut config,
                    &mut browser,
                    &mut active_puzzle,
                    &mut piece_q,
                    &hotspot_q,
                    &anim_q,
                    &mut audio_out,
                );
            },
        )
        .unwrap();
    step_app(app, &AppInput::default());
}

fn app_click(app: &mut App, x: i32, y: i32) {
    step_app(app, &AppInput {
        pointer_x: x,
        pointer_y: y,
        left_pressed: true,
        ..Default::default()
    });
    step_app(app, &AppInput {
        pointer_x: x,
        pointer_y: y,
        left_released: true,
        ..Default::default()
    });
}

fn app_press(app: &mut App, x: i32, y: i32) {
    step_app(app, &AppInput {
        pointer_x: x,
        pointer_y: y,
        left_pressed: true,
        ..Default::default()
    });
}

fn app_release(app: &mut App, x: i32, y: i32) {
    step_app(app, &AppInput {
        pointer_x: x,
        pointer_y: y,
        left_released: true,
        ..Default::default()
    });
}

fn app_held_group_pieces(app: &mut App) -> Vec<(usize, i32, i32)> {
    let mut items: Vec<(usize, usize, i32, i32)> = app
        .world_mut()
        .query::<(&PieceIdentity, &PieceLocation)>()
        .iter(app.world())
        .filter_map(|(id, loc)| match *loc {
            PieceLocation::Held {
                group_offset,
                group_order,
                ..
            } => Some((group_order, id.index, group_offset.0, group_offset.1)),
            _ => None,
        })
        .collect();
    items.sort_by_key(|&(ord, idx, _, _)| (ord, idx));
    items
        .into_iter()
        .map(|(_, idx, dx, dy)| (idx, dx, dy))
        .collect()
}

fn app_auto_solve(app: &mut App) {
    app_set_screen(app, AppScreen::Playing);
    step_app(app, &AppInput {
        f2: true,
        ..Default::default()
    });
}

fn app_trigger_hint(app: &mut App) {
    app_set_screen(app, AppScreen::Playing);
    step_app(app, &AppInput {
        f1: true,
        ..Default::default()
    });
}

fn app_open_setup(app: &mut App, return_to_playing: bool) {
    if return_to_playing {
        app_set_screen(app, AppScreen::Playing);
        step_app(app, &AppInput {
            f4: true,
            ..Default::default()
        });
    } else {
        app.world_mut().resource_mut::<SetupState>().menu.error = None;
        app_set_screen(app, AppScreen::Setup { return_to_playing: false });
    }
}

fn app_exit_setup(app: &mut App, return_to_playing: bool) {
    app_sync_prefs_from_setup(app);
    app_set_screen(
        app,
        if return_to_playing {
            AppScreen::Playing
        } else {
            AppScreen::Browser
        },
    );
}

fn app_open_in_game_menu(app: &mut App) {
    let menu = &mut app.world_mut().resource_mut::<InGameMenuState>();
    menu.open = true;
    menu.selected = 0;
    menu.pressed = None;
}

fn app_close_in_game_menu(app: &mut App, toggle_toolbox: bool) {
    {
        let menu = &mut app.world_mut().resource_mut::<InGameMenuState>();
        menu.open = false;
        menu.pressed = None;
    }
    if toggle_toolbox {
        let cur = app_meta(app).toolbox_open;
        app_meta_mut(app).toolbox_open = !cur;
    }
}

fn app_exec_in_game_menu(app: &mut App, cmd: crate::app::InGameMenuCommand) {
    app_open_in_game_menu(app);
    app.world_mut().resource_mut::<InGameMenuState>().selected = cmd as usize;
    step_app(app, &AppInput {
        enter: true,
        ..Default::default()
    });
}

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
        let mut app = test_browser();
        app_spawn_puzzle(&mut app, 1, asset, 3, true, false, false);
        let mut renderer = PuzzleRenderer::new();
        let original = app_meta(&app).full_picture_fb.pixels;
        app_set_piece_placed(&mut app, 0, true);
        app_set_piece_held(&mut app, 1);
        let mut fb = Framebuffer::new();
        renderer.render(app.world_mut(), &mut fb, &toolbox, &font, 160, 100);
        let mut before = vec![0; 320 * 200 * 4];
        fb.to_rgba8(&mut before);
        let mut after = before.clone();
        renderer.blend_picture_guide(app.world_mut(), &mut after, 160, 100);
        assert_ne!(before, after, "ega={ega}");
        let pfx = app_meta(&app).playfield_x();
        let pfy = app_meta(&app).playfield_y();
        for (index, held) in [(0, false), (1, true)] {
            let piece = app_piece_geom(&mut app, index);
            let sprite = app
                .world_mut()
                .query::<(&PieceIdentity, &PieceSprite)>()
                .iter(app.world())
                .find_map(|(id, s)| (id.index == index).then(|| s.sprite.clone()))
                .unwrap();
            let (x, y) = if held {
                (
                    160 - piece.base_w / 2 + piece.target_x - piece.base_x,
                    100 - piece.base_h / 2 + piece.target_y - piece.base_y,
                )
            } else {
                (pfx + piece.target_x, pfy + piece.target_y)
            };
            for dy in 0..i32::from(sprite.sub_h) {
                for dx in 0..i32::from(sprite.width) {
                    let source = (dy + i32::from(sprite.sub_y)) as usize
                        * usize::from(sprite.stride)
                        + (dx + i32::from(sprite.sub_x)) as usize;
                    if !(0..320).contains(&(x + dx))
                        || !(0..200).contains(&(y + dy))
                        || !sprite.is_opaque(source)
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
        assert_eq!(app_meta(&app).full_picture_fb.pixels, original);
        app_meta_mut(&mut app).solved = true;
        let mut solved = before.clone();
        renderer.blend_picture_guide(app.world_mut(), &mut solved, 160, 100);
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
        let mut app = test_browser();
        app_spawn_puzzle(&mut app, 1, asset, 2, true, false, false);
        let mut renderer = PuzzleRenderer::new();
        app_meta_mut(&mut app).solved = true;
        app_meta_mut(&mut app).elapsed_secs = 3661.0;
        let mut without_timer = Framebuffer::new();
        renderer.render(app.world_mut(), &mut without_timer, &toolbox, &font, 160, 100);
        app_meta_mut(&mut app).timer_enabled = true;
        let mut with_timer = Framebuffer::new();
        renderer.render(app.world_mut(), &mut with_timer, &toolbox, &font, 160, 100);
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

fn test_browser() -> App {
    let game_dir = test_game_dir();
    let prefs = Preferences {
        music_on: 0,
        sfx_on: 0,
        ..Preferences::default()
    };
    build_game_app_from_parts(
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
    app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 1;
    app_start_selected_puzzle(&mut app);
    let tray = app_tray_order(&mut app);
    let first = tray[0];
    let held = tray[1];
    app_set_piece_placed(&mut app, first, true);
    app_set_piece_held(&mut app, held);
    let order = app_tray_order(&mut app);
    app_set_screen(&mut app, AppScreen::Browser);
    {
        let b = &mut app.world_mut().resource_mut::<BrowserState>();
        b.curved_edges = !b.curved_edges;
    }
    assert!(app_can_resume_selected(&app));
    app_start_selected_puzzle(&mut app);
    let game_dir = app.world().resource::<GameConfig>().game_dir.clone();
    let ega = PuzzleAsset::load(&game_dir, 1, true).unwrap();
    app_replace_scene_asset(&mut app, ega);
    assert_eq!(app_meta(&app).grid_dim, 1);
    assert!(app_is_placed(&mut app, first));
    assert_eq!(app_held_piece(&mut app), Some(held));
    assert_eq!(app_tray_order(&mut app), order);
    assert!(app_meta(&app).asset.uses_ega());
    assert!((11..=16).contains(&app_piece_count(&mut app)));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn common_captions_render_and_respect_text_preference() {
    let mut app = test_browser();
    let mut renderer = AppRenderer::new();
    let game_dir = app.world().resource::<GameConfig>().game_dir.clone();
    for ega in [false, true] {
        app.world_mut().resource_mut::<BrowserState>().preview_asset =
            PuzzleAsset::load(&game_dir, 1, ega).unwrap();
        app.world_mut().resource_mut::<GameConfig>().prefs.text_msg_on = 0;
        renderer.render(app.world_mut(), 160, 100);
        let baseline = renderer.fb.clone();
        app.world_mut().non_send_mut::<AudioEngine>().play_common(CommonSound::ChooseSize);
        renderer.render(app.world_mut(), 160, 100);
        let hidden = renderer.fb.clone();
        assert_eq!(baseline.pixels, hidden.pixels);
        app.world_mut().resource_mut::<GameConfig>().prefs.text_msg_on = 1;
        renderer.render(app.world_mut(), 160, 100);
        let shown = renderer.fb.clone();
        assert_ne!(baseline.pixels, shown.pixels);
        assert_eq!(&baseline.pixels[164 * 320..], &shown.pixels[164 * 320..]);
        app.world_mut().non_send_mut::<AudioEngine>().update(60.0);
        renderer.render(app.world_mut(), 160, 100);
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
    app.world_mut().resource_mut::<GameConfig>().prefs.text_msg_on = 1;
    app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 1;
    for ega in [false, true] {
        app.world_mut().resource_mut::<GameConfig>().prefs.video_mode = if ega { 2 } else { 0 };
        for num in 1..=15 {
            app.world_mut().resource_mut::<BrowserState>().selected_puzzle = num;
            app_restart_selected_puzzle(&mut app);
            app_click(&mut app, 20, 40);
            let held = app_held_piece(&mut app).unwrap();
            let piece = app_piece_geom(&mut app, held);
            let (x, y) = (
                app_meta(&app).playfield_x() + piece.base_x + piece.base_w / 2,
                app_meta(&app).playfield_y() + piece.base_y + piece.base_h / 2,
            );
            app_click(&mut app, x, y);
            renderer.render(app.world_mut(), 0, 0);
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
    app_start_selected_puzzle(&mut app);
    app_click(&mut app, 20, 40);
    let placed = app_held_piece(&mut app).unwrap();
    let piece = app_piece_geom(&mut app, placed);
    let target = (
        app_meta(&app).playfield_x() + piece.base_x + piece.base_w / 2,
        app_meta(&app).playfield_y() + piece.base_y + piece.base_h / 2,
    );
    app_click(&mut app, target.0, target.1);
    app_click(&mut app, 20, 40);
    app_meta_mut(&mut app).elapsed_secs = 123.0;
    app_meta_mut(&mut app).tray_scroll = 1;
    app_meta_mut(&mut app).toolbox_open = false;
    let held = app_held_piece(&mut app);
    if let Some(hidx) = held {
        let he = app
            .world_mut()
            .query::<(Entity, &PieceIdentity)>()
            .iter(app.world())
            .find_map(|(e, id)| (id.index == hidx).then_some(e))
            .unwrap();
        app.world_mut()
            .entity_mut(he)
            .insert(HintHighlight::new(1.0));
    }
    let tray = app_tray_order(&mut app);
    for mode in [2, 1] {
        app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = mode;
        app_sync_prefs_from_setup(&mut app);
        assert_eq!(
            app_meta(&app).asset.sprites[0].bpp,
            if mode == 2 { 4 } else { 8 }
        );
        assert!(app_is_placed(&mut app, placed));
        assert_eq!(app_held_piece(&mut app), held);
        assert_eq!(app_tray_order(&mut app), tray);
        assert_eq!(app_meta(&app).tray_scroll, 1);
        assert!(!app_meta(&app).toolbox_open);
        let hint = app
            .world_mut()
            .query::<(&PieceIdentity, &HintHighlight)>()
            .iter(app.world())
            .next()
            .map(|(id, h)| (id.index, h.timer.remaining_secs()));
        assert_eq!(hint, held.map(|index| (index, 1.0)));
        assert_eq!(app_meta(&app).elapsed_secs, 123.0);
        assert!(app_can_resume_selected(&app));
    }
    let palette = app_meta(&app).asset.palette;
    app.world_mut().resource_mut::<GameConfig>().game_dir = PathBuf::from("/dev/null");
    app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = 2;
    app_sync_prefs_from_setup(&mut app);
    assert!(app.world().resource::<ScreenState>().error.is_some());
    assert_eq!(app.world().resource::<GameConfig>().prefs.video_mode, 1);
    assert_eq!(
        app.world().resource::<SetupState>().menu.prefs.video_mode,
        1
    );
    assert_eq!(app_meta(&app).asset.palette, palette);
    assert_eq!(app_held_piece(&mut app), held);
    assert!(app_is_placed(&mut app, placed));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn setup_video_change_keeps_solved_state_and_supports_ega_fallback() {
    let mut app = test_browser();
    app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 6;
    app_start_selected_puzzle(&mut app);
    step_app(&mut app, &AppInput {
        f2: true,
        ..Default::default()
    });
    assert!(
        app.world_mut()
            .query::<&AnimationState>()
            .iter(app.world())
            .next()
            .is_some()
    );
    app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = 2;
    app_sync_prefs_from_setup(&mut app);
    assert!(app_meta(&app).solved);
    assert!(app_all_placed(&mut app));
    assert!(
        app.world_mut()
            .query::<&AnimationState>()
            .iter(app.world())
            .next()
            .is_none()
    );
    assert_eq!(app_meta(&app).asset.sprites[0].bpp, 8);
    assert!(app.world().resource::<GameConfig>().session_use_ega);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn video_switch_is_atomic_when_preview_and_paused_puzzle_differ() {
    let mut app = test_browser();
    app_start_selected_puzzle(&mut app);
    app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 2;
    {
        let game_dir = app.world().resource::<GameConfig>().game_dir.clone();
        let use_ega = app.world().resource::<GameConfig>().session_use_ega;
        app.world_mut().resource_mut::<BrowserState>().preview_asset =
            PuzzleAsset::load(&game_dir, 2, use_ega).unwrap();
    }
    let old_preview = app.world().resource::<BrowserState>().preview_asset.palette;
    let old_active = app_meta(&app).asset.palette;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mickey-video-{unique}"));
    std::fs::create_dir(&dir).unwrap();
    let orig_dir = app.world().resource::<GameConfig>().game_dir.clone();
    std::fs::copy(orig_dir.join("P02.PZ4"), dir.join("P02.PZ4")).unwrap();
    app.world_mut().resource_mut::<GameConfig>().game_dir = dir.clone();
    app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = 2;
    app_sync_prefs_from_setup(&mut app);
    app.world_mut().resource_mut::<GameConfig>().game_dir = test_game_dir();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(app.world().resource::<ScreenState>().error.is_some());
    assert_eq!(app.world().resource::<GameConfig>().prefs.video_mode, 1);
    assert_eq!(
        app.world().resource::<BrowserState>().preview_asset.palette,
        old_preview
    );
    assert_eq!(app_meta(&app).asset.palette, old_active);
    assert!(!app.world().resource::<GameConfig>().session_use_ega);
    app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = 2;
    app_sync_prefs_from_setup(&mut app);
    assert!(app.world().resource::<ScreenState>().error.is_none());
    assert_eq!(app.world().resource::<BrowserState>().preview_asset.id, 2);
    assert_eq!(
        app.world().resource::<BrowserState>().preview_asset.sprites[0].bpp,
        4
    );
    assert_eq!(app_meta(&app).puzzle_num, 1);
    assert_eq!(app_meta(&app).asset.sprites[0].bpp, 4);
    app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 1;
    {
        let game_dir = app.world().resource::<GameConfig>().game_dir.clone();
        let use_ega = app.world().resource::<GameConfig>().session_use_ega;
        app.world_mut().resource_mut::<BrowserState>().preview_asset =
            PuzzleAsset::load(&game_dir, 1, use_ega).unwrap();
    }
    assert!(app_can_resume_selected(&app));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_resume_preserves_board_tray_held_piece_and_time() {
    let mut app = test_browser();
    app_start_selected_puzzle(&mut app);
    app_click(&mut app, 20, 40);
    let placed = app_held_piece(&mut app).unwrap();
    let piece = app_piece_geom(&mut app, placed);
    let target = (
        app_meta(&app).playfield_x() + piece.base_x + piece.base_w / 2,
        app_meta(&app).playfield_y() + piece.base_y + piece.base_h / 2,
    );
    app_click(&mut app, target.0, target.1);
    app_click(&mut app, 20, 40);
    app_meta_mut(&mut app).elapsed_secs = 123.0;
    let held = app_held_piece(&mut app);
    let tray = app_tray_order(&mut app);
    app_set_screen(&mut app, AppScreen::Browser);
    assert!(app_can_resume_selected(&app));
    app_start_selected_puzzle(&mut app);
    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(app_is_placed(&mut app, placed));
    assert_eq!(app_held_piece(&mut app), held);
    assert_eq!(app_tray_order(&mut app), tray);
    assert_eq!(app_meta(&app).elapsed_secs, 123.0);
    app.world_mut().resource_mut::<GameConfig>().game_dir = PathBuf::from("/dev/null");
    app_restart_selected_puzzle(&mut app);
    assert!(app.world().resource::<ScreenState>().error.is_some());
    assert!(app_is_placed(&mut app, placed));
    assert_eq!(app_held_piece(&mut app), held);
    app.world_mut().resource_mut::<GameConfig>().game_dir = test_game_dir();
    app_restart_selected_puzzle(&mut app);
    assert!(!app_all_placed(&mut app));
    assert_eq!(app_held_piece(&mut app), None);
    assert_eq!(app_tray_order(&mut app).len(), 9);
    assert_eq!(app_meta(&app).elapsed_secs, 0.0);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_only_resumes_matching_unfinished_puzzles() {
    let mut app = test_browser();
    assert!(!app_can_resume_selected(&app));
    app_start_selected_puzzle(&mut app);
    assert!(app_can_resume_selected(&app));
    app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 2;
    assert!(!app_can_resume_selected(&app));
    app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 1;
    app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 4;
    assert!(!app_can_resume_selected(&app));
    app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 3;
    app.world_mut().resource_mut::<BrowserState>().curved_edges = false;
    assert!(!app_can_resume_selected(&app));
    app.world_mut().resource_mut::<BrowserState>().curved_edges = true;
    app.world_mut().resource_mut::<GameConfig>().prefs.video_mode = 2;
    assert!(!app_can_resume_selected(&app));
    app.world_mut().resource_mut::<GameConfig>().prefs.video_mode = 1;
    app_meta_mut(&mut app).solved = true;
    assert!(!app_can_resume_selected(&app));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn every_puzzle_can_start_in_debug_builds() {
    let mut app = test_browser();
    for num in 1..=15 {
        let asset = PuzzleAsset::load(&test_game_dir(), num, false).unwrap();
        app_spawn_puzzle(&mut app, num, asset, 3, true, true, true);
        assert_eq!(app_piece_count(&mut app), 9);
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
    let mut app = test_browser();
    for ega in [false, true] {
        for num in 1..=15 {
            let asset = PuzzleAsset::load(&test_game_dir(), num, ega).unwrap();
            for dim in 2..=8 {
                for curved in [false, true] {
                    app_spawn_puzzle(&mut app, num, asset.clone(), dim, curved, true, true);
                    let full_picture_fb = app_meta(&app).full_picture_fb.clone();
                    let mut coverage = vec![0u8; 270 * 196];
                    let mut q = app.world_mut().query::<(&PieceGeometry, &PieceSprite)>();
                    for (geom, spr) in q.iter(app.world()) {
                        for y in 0..spr.sprite.sub_h as i32 {
                            for x in 0..spr.sprite.width as i32 {
                                let color =
                                    spr.sprite.pixels[(y * spr.sprite.width as i32 + x) as usize];
                                if spr
                                    .sprite
                                    .is_opaque((y * spr.sprite.width as i32 + x) as usize)
                                {
                                    let px = geom.target_x + x;
                                    let py = geom.target_y + y;
                                    assert!((0..270).contains(&px) && (0..196).contains(&py));
                                    coverage[(py * 270 + px) as usize] += 1;
                                    assert_eq!(
                                        color,
                                        full_picture_fb.get_pixel(2 + px, 2 + py),
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
    let mut app = test_browser();
    let dir = test_game_dir();
    let asset = PuzzleAsset::load(&dir, 1, false).expect("P01.PZ8 should load");
    let tbx = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).expect("MJP.TBX should load");
    let font = FontAsset::load(&dir.join("MJP.NFT")).expect("MJP.NFT should load");

    for dim in 2..=8 {
        for &curved in &[false, true] {
            app_spawn_puzzle(&mut app, 1, asset.clone(), dim, curved, true, true);
            assert_eq!(app_piece_count(&mut app), dim * dim);
            assert_eq!(app_tray_order(&mut app).len(), dim * dim);

            // Verify every cut piece has non-zero pixels
            let mut q = app.world_mut().query::<(&PieceIdentity, &PieceSprite)>();
            for (ident, spr) in q.iter(app.world()) {
                let non_zero = spr.sprite.pixels.iter().filter(|&&px| px != 0).count();
                assert!(
                    non_zero > 20,
                    "Piece {} in dim={} curved={} has too few opaque pixels ({})",
                    ident.index,
                    dim,
                    curved,
                    non_zero
                );
            }

            let mut fb = Framebuffer::new();
            let mut renderer = PuzzleRenderer::new();
            renderer.render(app.world_mut(), &mut fb, &tbx, &font, 160, 100);
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
    let mut app = test_browser();
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
                    app_spawn_puzzle(&mut app, num, asset.clone(), dim, curved, true, true);
                    app_set_screen(&mut app, AppScreen::Playing);
                    let pf_x = app_meta(&app).playfield_x();
                    let pf_y = app_meta(&app).playfield_y();
                    for step in 0..dim * dim {
                        app_press(&mut app, 20, 40);
                        let index = app_held_piece(&mut app).expect("tray pick");
                        let geom = app_piece_geom(&mut app, index);
                        let x = pf_x + geom.base_x + geom.base_w / 2;
                        let y = pf_y + geom.base_y + geom.base_h / 2;
                        if step % 2 == 0 {
                            app_release(&mut app, 21, 41);
                            assert_eq!(app_held_piece(&mut app), Some(index));
                            app_press(&mut app, x, y);
                        }
                        app_release(&mut app, x, y);
                        assert!(
                            app_is_placed(&mut app, index),
                            "P{num:02} ega={ega} grid={dim} curved={curved} piece={index}"
                        );
                        assert!(app_held_piece(&mut app).is_none());
                        placed += 1;
                    }
                    assert!(app_meta(&app).solved);
                    assert!(
                        app.world().non_send::<AudioEngine>().common_caption().is_none(),
                        "stale pickup caption after P{num:02}"
                    );
                    assert!(app_tray_order(&mut app).is_empty());
                    assert!(app_all_placed(&mut app));
                    let mut fb = Framebuffer::new();
                    let mut renderer = PuzzleRenderer::new();
                    renderer.render(app.world_mut(), &mut fb, &toolbox, &font, 160, 100);
                    assert!(app_active_anim(&mut app).is_some());
                    app_click(&mut app, 160, 100);
                    assert!(app_active_anim(&mut app).is_none());
                    renderer.render(app.world_mut(), &mut fb, &toolbox, &font, 160, 100);
                    app_click(&mut app, 2, 2);
                    assert!(app.world().resource::<InGameMenuState>().open);
                    app_close_in_game_menu(&mut app, false);
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
    let mut app = test_browser();
    let mut asset = PuzzleAsset::load(&test_game_dir(), 1, false).unwrap();
    asset.records = vec![
        assets::SceneRecord::Frame {
            bg_frame_idx: 0,
            bg_color: 0,
            commands: Vec::new(),
        };
        3
    ];
    app_spawn_puzzle(&mut app, 1, asset, 3, true, true, true);
    app_set_screen(&mut app, AppScreen::Playing);
    app.world_mut().spawn(AnimationState {
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
    let frame_secs = app_active_anim(&mut app).unwrap().frame_delay_secs;
    step_app(&mut app, &AppInput {
        dt: 2.5 * frame_secs,
        ..Default::default()
    });
    let anim = app_active_anim(&mut app).unwrap();
    assert_eq!(anim.cur_frame, 2);
    assert!((anim.tick_accum - 0.5 * frame_secs).abs() < 0.00001);
    {
        let mut q = app.world_mut().query::<&mut AnimationState>();
        let mut anim = q.iter_mut(app.world_mut()).next().unwrap();
        anim.cur_frame = 0;
        anim.tick_accum = 0.0;
        anim.repetitions_left = 2;
    }
    step_app(&mut app, &AppInput {
        dt: 4.5 * frame_secs,
        ..Default::default()
    });
    let anim = app_active_anim(&mut app).unwrap();
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
    app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = 2;
    app_sync_prefs_from_setup(&mut app);
    for id in [1, 6, 7, 10] {
        app.world_mut().resource_mut::<BrowserState>().selected_puzzle = id;
        app_restart_selected_puzzle(&mut app);
        app.world_mut().non_send_mut::<AudioEngine>().update(60.0);
        let bpp = app_meta(&app).asset.sprites[0].bpp;
        let game_dir = app.world().resource::<GameConfig>().game_dir.clone();
        let toolbox = ToolboxAsset::load(&game_dir.join("MJP.TBX"), usize::from(bpp == 4)).unwrap();
        let font = &app.world().resource::<StaticAssets>().font.clone();
        let mut expected = Framebuffer::new();
        let mut puzzle_renderer = PuzzleRenderer::new();
        puzzle_renderer.render(app.world_mut(), &mut expected, &toolbox, &font, 160, 100);
        let mut app_renderer = AppRenderer::new();
        app_renderer.render(app.world_mut(), 160, 100);
        let actual = &app_renderer.fb;
        assert!(
            actual.pixels == expected.pixels,
            "P{id:02}, toolbox variant {}",
            usize::from(bpp == 4)
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
        let mut app = test_browser();
        let asset = PuzzleAsset::load(&dir, 1, ega).unwrap();
        let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(ega)).unwrap();
        app_spawn_puzzle(&mut app, 1, asset, 3, true, true, true);
        let mut puzzle_renderer = PuzzleRenderer::new();
        let mut fb = Framebuffer::new();
        puzzle_renderer.render(app.world_mut(), &mut fb, &toolbox, &font, 160, 100);
        assert_eq!(fb.get_pixel(1, 100), fb.color_index([0; 3]), "ega={ega}");
        assert_eq!(fb.get_pixel(3, 19), fb.color_index([85; 3]), "ega={ega}");
        assert_ne!(fb.get_pixel(1, 100), fb.get_pixel(3, 19), "ega={ega}");
        let mut app = test_browser();
        app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = if ega { 2 } else { 1 };
        app_sync_prefs_from_setup(&mut app);
        let mut app_renderer = AppRenderer::new();
        app_renderer.render(app.world_mut(), 160, 100);
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
    assert!(crate::puzzle::export_print_pbm(&temp, 1).is_ok());
    let image = std::fs::read(temp.join("prints/P01.pbm")).unwrap();
    assert_eq!(&image[..11], b"P4\n320 200\n");
    assert_eq!(&image[11..], &PrintAsset::load(&source, 1).unwrap().bitmap);
    std::fs::remove_dir_all(temp.join("prints")).unwrap();
    std::fs::write(temp.join("prints"), b"blocked").unwrap();
    let err = crate::puzzle::export_print_pbm(&temp, 1).unwrap_err();
    assert!(err.starts_with("Print error:"));
    std::fs::remove_dir_all(temp).unwrap();
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_navigation_wraps_and_buttons_align() {
    let mut app = build_game_app(test_game_dir(), true).unwrap();
    app_set_screen(&mut app, AppScreen::Browser);
    app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 1;
    let prev_btn = BROWSER_BUTTONS[0];
    let next_btn = BROWSER_BUTTONS[1];
    app_click(&mut app, prev_btn.x + prev_btn.w / 2, prev_btn.y + prev_btn.h / 2);
    assert_eq!(app.world().resource::<BrowserState>().selected_puzzle, 15);
    app_click(&mut app, next_btn.x + next_btn.w / 2, next_btn.y + next_btn.h / 2);
    assert_eq!(app.world().resource::<BrowserState>().selected_puzzle, 1);

    app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 1;
    step_app(&mut app, &AppInput {
        down: true,
        ..Default::default()
    });
    assert_eq!(app.world().resource::<BrowserState>().selected_grid_dim, 2);
    step_app(&mut app, &AppInput {
        up: true,
        ..Default::default()
    });
    assert_eq!(app.world().resource::<BrowserState>().selected_grid_dim, 1);
    step_app(&mut app, &AppInput {
        up: true,
        ..Default::default()
    });
    assert_eq!(app.world().resource::<BrowserState>().selected_grid_dim, 8);
    step_app(&mut app, &AppInput {
        down: true,
        ..Default::default()
    });
    assert_eq!(app.world().resource::<BrowserState>().selected_grid_dim, 1);

    app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 3;
    let edges_was = app.world().resource::<BrowserState>().curved_edges;
    let edges_btn = BROWSER_BUTTONS[3];
    app_click(&mut app, edges_btn.x + edges_btn.w / 2, edges_btn.y + edges_btn.h / 2);
    assert_ne!(app.world().resource::<BrowserState>().curved_edges, edges_was);

    let setup_btn = BROWSER_BUTTONS[4];
    app_click(&mut app, setup_btn.x + setup_btn.w / 2, setup_btn.y + setup_btn.h / 2);
    assert_eq!(
        app_screen(&app),
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
    app.world_mut().resource_mut::<SetupState>().menu.error = Some("stale error".to_string());
    app_open_setup(&mut app, false);
    assert!(app.world().resource::<SetupState>().menu.error.is_none());
    assert_eq!(
        app_screen(&app),
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

    let mut app = test_browser();
    let mut preset_app = test_browser();
    let asset = PuzzleAsset::load(&dir, 1, false).unwrap();
    let t1 = Instant::now();
    app_spawn_puzzle(&mut app, 1, asset.clone(), 8, true, true, true);
    let cut_8x8 = t1.elapsed();

    let t1_preset = Instant::now();
    app_spawn_puzzle(&mut preset_app, 1, asset.clone(), 1, true, true, true);
    let cut_preset = t1_preset.elapsed();

    let tbx = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).unwrap();
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    let mut renderer = PuzzleRenderer::new();
    let mut preset_renderer = PuzzleRenderer::new();
    let mut fb = Framebuffer::new();
    let mut rgba = vec![0u8; 320 * 200 * 4];

    // Warm up caches
    renderer.render(app.world_mut(), &mut fb, &tbx, &font, 160, 100);
    fb.to_rgba8(&mut rgba);
    renderer.blend_picture_guide(app.world_mut(), &mut rgba, 160, 100);

    let iters = 2000;
    let t2 = Instant::now();
    for _ in 0..iters {
        renderer.render(app.world_mut(), &mut fb, &tbx, &font, 160, 100);
        black_box(&fb.pixels[0]);
    }
    let render_warm = t2.elapsed();

    // Dragging a piece
    app_set_piece_held(&mut app, 0);
    let t_drag = Instant::now();
    for i in 0..iters {
        let mx = 80 + (i % 160);
        let my = 40 + (i % 120);
        renderer.render(app.world_mut(), &mut fb, &tbx, &font, mx, my);
        fb.to_rgba8(&mut rgba);
        renderer.blend_picture_guide(app.world_mut(), &mut rgba, mx, my);
        black_box(rgba[0]);
    }
    let drag_full = t_drag.elapsed();
    app_set_piece_placed(&mut app, 0, false);

    let t3 = Instant::now();
    for _ in 0..iters {
        fb.to_rgba8(&mut rgba);
        black_box(rgba[0]);
    }
    let to_rgba = t3.elapsed();

    let t4 = Instant::now();
    for _ in 0..iters {
        renderer.blend_picture_guide(app.world_mut(), &mut rgba, 160, 100);
        black_box(rgba[0]);
    }
    let guide = t4.elapsed();

    // Solved idle & animation/dissolve
    app_auto_solve(&mut preset_app);
    let anim_iters = 500;
    let t_anim = Instant::now();
    for _ in 0..anim_iters {
        preset_renderer.render(preset_app.world_mut(), &mut fb, &tbx, &font, 160, 100);
        black_box(&fb.pixels[0]);
    }
    let render_anim = t_anim.elapsed();

    {
        let w = preset_app.world_mut();
        if let Some(mut anim) = w.query::<&mut AnimationState>().iter_mut(w).next() {
            anim.transition_mode = 8;
            anim.tick_accum = anim.frame_delay_secs * 0.25;
        }
    }
    let t_dissolve = Instant::now();
    for _ in 0..anim_iters {
        preset_renderer.render(preset_app.world_mut(), &mut fb, &tbx, &font, 160, 100);
        black_box(&fb.pixels[0]);
    }
    let render_dissolve = t_dissolve.elapsed();

    // Dragging a piece with the new layer-composited pipeline:
    // 1) CPU-only cache-hit cost (offset/key lookup when board/toolbox/held layers are cached)
    // 2) Actual headless SDL3 software-renderer Canvas (clear + 3x texture copy + present)
    app_set_piece_held(&mut app, 0);
    let mut layer_board = vec![0u8; 320 * 200 * 4];
    let mut layer_tbx = Vec::new();
    let mut layer_held = Vec::new();
    renderer.render_board_layer_rgba(app.world_mut(), &tbx, true, &mut layer_board);
    let tbx_bounds =
        renderer.render_toolbox_layer_rgba(app.world_mut(), &tbx, &font, 160, 100, &mut layer_tbx);
    let held_bounds = renderer
        .render_held_group_layer_rgba(app.world_mut(), 160, 100, &mut layer_held)
        .unwrap();
    let pf_x = app_meta(&app).playfield_x();
    let pf_y = app_meta(&app).playfield_y();
    let anchor = app_piece_geom(&mut app, 0);
    let t_drag_cache_hit = Instant::now();
    for i in 0..iters {
        let mx = 80 + (i % 160);
        let my = 40 + (i % 120);
        let anchor_cell = app_meta(&app).grid_cell_at(mx - pf_x, my - pf_y);
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
    app_set_piece_placed(&mut app, 0, false);

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
    let mut app = test_browser();
    let asset = PuzzleAsset::load(&test_game_dir(), 1, false).unwrap();
    app_spawn_puzzle(&mut app, 1, asset, 4, true, true, true);

    // Moving the cursor around the board without a held piece does not change the signature
    let idle_a = crate::puzzle_render::puzzle_present_key(app.world_mut(), 120, 80, true);
    let idle_b = crate::puzzle_render::puzzle_present_key(app.world_mut(), 200, 140, true);
    assert_eq!(idle_a, idle_b);

    // Moving over a tray slot changes the signature (hover highlight)
    let tray_hover = crate::puzzle_render::puzzle_present_key(app.world_mut(), 20, 40, true);
    assert_ne!(idle_a, tray_hover);

    // Toggling picture guide changes the signature
    let no_guide = crate::puzzle_render::puzzle_present_key(app.world_mut(), 120, 80, false);
    assert_ne!(idle_a, no_guide);

    // Holding a piece tracks cursor position changes
    app_set_piece_held(&mut app, 0);
    let held_a = crate::puzzle_render::puzzle_present_key(app.world_mut(), 120, 80, true);
    let held_b = crate::puzzle_render::puzzle_present_key(app.world_mut(), 124, 80, true);
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
    let mut app = build_game_app(dir, true).unwrap();
    app_set_screen(&mut app, AppScreen::Browser);
    app.world_mut().resource_mut::<GameConfig>().prefs.text_msg_on = 1;

    let idle_sig = present_signature(app.world_mut(), 120, 80, false, false);
    assert!(idle_sig.is_some());

    app.world_mut().non_send_mut::<AudioEngine>().play_common(CommonSound::NextPicture);
    { let mut s = AudioStatus::default(); sync_audio_status(app.world().non_send::<AudioEngine>(), &mut s); *app.world_mut().resource_mut::<AudioStatus>() = s; }
    let next_sig = present_signature(app.world_mut(), 120, 80, false, false);
    assert_ne!(idle_sig, next_sig);

    app.world_mut().non_send_mut::<AudioEngine>().play_common(CommonSound::StartPuzzle);
    { let mut s = AudioStatus::default(); sync_audio_status(app.world().non_send::<AudioEngine>(), &mut s); *app.world_mut().resource_mut::<AudioStatus>() = s; }
    let start_sig = present_signature(app.world_mut(), 120, 80, false, false);
    assert_ne!(next_sig, start_sig);

    // Advance enough time for the common caption to expire on an idle Browser screen
    for _ in 0..20 {
        app.world_mut().non_send_mut::<AudioEngine>().update(0.5);
    }
    { let mut s = AudioStatus::default(); sync_audio_status(app.world().non_send::<AudioEngine>(), &mut s); *app.world_mut().resource_mut::<AudioStatus>() = s; }
    assert!(app.world().non_send::<AudioEngine>().common_caption().is_none());
    let expired_sig = present_signature(app.world_mut(), 120, 80, false, false);
    assert_eq!(expired_sig, idle_sig);
}

fn synthetic_app() -> App {
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
    build_game_app_from_parts(
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
    app_set_screen(&mut app, AppScreen::Title);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, false, true));
    assert!(!renderer.render_if_needed(app.world_mut(), 120, 100, false, true));

    step_app(&mut app, &AppInput {
        enter: true,
        ..AppInput::default()
    });
    assert_eq!(app_screen(&app), AppScreen::Browser);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, false, true));
    assert!(!renderer.render_if_needed(app.world_mut(), 140, 120, false, true));

    // Common caption invalidates Browser signature and expires back to idle signature
    let idle_sig = present_signature(app.world_mut(), 100, 100, false, false);
    app.world_mut().non_send_mut::<AudioEngine>().play_common(CommonSound::NextPicture);
    { let mut s = AudioStatus::default(); sync_audio_status(app.world().non_send::<AudioEngine>(), &mut s); *app.world_mut().resource_mut::<AudioStatus>() = s; }
    assert_ne!(present_signature(app.world_mut(), 100, 100, false, false), idle_sig);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, false, false));

    for _ in 0..20 {
        step_app(&mut app, &AppInput {
            dt: 0.5,
            ..AppInput::default()
        });
    }
    assert!(app.world().non_send::<AudioEngine>().common_caption().is_none());
    assert_eq!(present_signature(app.world_mut(), 100, 100, false, false), idle_sig);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, false, false));

    // Enter Playing with a synthetic session, then F4 pauses into Setup, Esc resumes Playing
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, true);
    app_set_screen(&mut app, AppScreen::Playing);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, false, true));
    assert!(!renderer.render_if_needed(app.world_mut(), 150, 120, false, true));

    step_app(&mut app, &AppInput {
        f4: true,
        ..AppInput::default()
    });
    assert_eq!(
        app_screen(&app),
        AppScreen::Setup {
            return_to_playing: true
        }
    );
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, false, true));

    // Setup menu navigation updates revision and redraws
    step_app(&mut app, &AppInput {
        down: true,
        ..AppInput::default()
    });
    assert_eq!(app.world().resource::<SetupState>().menu.selected_row, 1);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, false, true));

    // Esc exits Setup back to Playing
    step_app(&mut app, &AppInput {
        escape: true,
        ..AppInput::default()
    });
    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, false, true));
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
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();

    app_spawn_puzzle(&mut app, 1, preview.clone(), 2, true, false, true);
    app_set_screen(&mut app, AppScreen::Playing);
    let sig1 = present_signature(app.world_mut(), 100, 100, true, false);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, true, false));
    assert!(!renderer.render_if_needed(app.world_mut(), 120, 120, true, false));

    // Replacing the session with the same puzzle_num and video mode must invalidate Reference.
    app_spawn_puzzle(&mut app, 1, preview.clone(), 3, false, false, true);
    let sig2 = present_signature(app.world_mut(), 100, 100, true, false);
    assert_ne!(sig2, sig1);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, true, false));

    // Replacing the scene asset on the same session must also invalidate Reference.
    let mut alt_asset = preview;
    alt_asset.palette[1] = [200, 100, 50];
    app_replace_scene_asset(&mut app, alt_asset);
    let sig3 = present_signature(app.world_mut(), 100, 100, true, false);
    assert_ne!(sig3, sig2);
    assert!(renderer.render_if_needed(app.world_mut(), 100, 100, true, false));

    let mut fresh = AppRenderer::new();
    assert!(fresh.render_if_needed(app.world_mut(), 100, 100, true, false));
    assert_eq!(renderer.rgba_buf, fresh.rgba_buf);
}

#[test]
fn title_escape_skips_directly_to_browser() {
    let mut app = synthetic_app();
    app_set_screen(&mut app, AppScreen::Title);
    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });
    assert_eq!(app_screen(&app), AppScreen::Browser);
}

#[test]
fn playing_escape_exits_without_updating_session_on_exit_frame() {
    let mut app = synthetic_app();
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, true);
    app_set_screen(&mut app, AppScreen::Playing);

    step_app(&mut app, &AppInput {
        dt: 1.0,
        escape: true,
        f2: true,
        ..Default::default()
    });

    assert_eq!(app_screen(&app), AppScreen::Browser);
    let meta = app_meta(&app);
    assert!(!meta.solved);
    assert_eq!(meta.elapsed_secs, 0.0);
}

#[test]
fn title_and_credits_ignore_non_finite_and_negative_dt() {
    let mut app = synthetic_app();
    app_set_screen(&mut app, AppScreen::Title);
    for dt in [f32::NAN, f32::NEG_INFINITY, -5.0] {
        step_app(&mut app, &AppInput {
            dt,
            ..Default::default()
        });
        assert_eq!(app_screen(&app), AppScreen::Title);
    }
    step_app(&mut app, &AppInput {
        dt: 8.1,
        ..Default::default()
    });
    assert_eq!(app_screen(&app), AppScreen::Browser);

    app_set_screen(&mut app, AppScreen::Credits { page: 0 });
    for dt in [f32::NAN, f32::NEG_INFINITY, -5.0] {
        step_app(&mut app, &AppInput {
            dt,
            ..Default::default()
        });
        assert_eq!(app_screen(&app), AppScreen::Credits { page: 0 });
    }
    step_app(&mut app, &AppInput {
        dt: 5.1,
        ..Default::default()
    });
    assert_eq!(app_screen(&app), AppScreen::Browser);
}

#[test]
fn enter_on_frame_animation_completes_naturally_does_not_restart_or_spawn_pointer() {
    let mut app = synthetic_app();
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, false);
    app_set_screen(&mut app, AppScreen::Playing);
    app_auto_solve(&mut app);
    assert!(app_active_anim(&mut app).is_some());
    assert!(app_meta(&app).keyboard_pointer.is_none());

    step_app(&mut app, &AppInput {
        dt: 10.0,
        enter: true,
        ..Default::default()
    });

    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(app_active_anim(&mut app).is_none());
    assert!(app_meta(&app).keyboard_pointer.is_none());
}

#[test]
fn setup_left_on_save_and_return_does_not_activate_save() {
    let mut app = synthetic_app();
    app_open_setup(&mut app, false);
    app.world_mut().resource_mut::<SetupState>().menu.selected_row = 6;
    step_app(&mut app, &AppInput {
        left: true,
        ..Default::default()
    });
    assert_eq!(
        app_screen(&app),
        AppScreen::Setup {
            return_to_playing: false
        }
    );
    assert!(app.world().resource::<SetupState>().menu.error.is_none());
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
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, false);
    app_set_piece_placed(&mut app, 0, true);
    app_set_screen(&mut app, AppScreen::Playing);

    // First Escape asks for confirmation instead of discarding progress.
    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });
    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(app.world().resource::<InGameMenuState>().confirm_exit_puzzle);
    assert!(
        app_meta(&app)
            .status_message()
            .is_some_and(|m| m.contains("We're not done yet"))
    );

    // Second Escape cancels the confirmation and resumes playing.
    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });
    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(!app.world().resource::<InGameMenuState>().confirm_exit_puzzle);

    // Escape then Enter confirms exiting to Browser.
    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(app.world().resource::<InGameMenuState>().confirm_exit_puzzle);
    step_app(&mut app, &AppInput {
        enter: true,
        ..Default::default()
    });
    assert_eq!(app_screen(&app), AppScreen::Browser);
    assert!(!app.world().resource::<InGameMenuState>().confirm_exit_puzzle);
}

#[test]
fn in_game_hotkeys_toggle_music_sfx_timer_and_text_messages() {
    let mut app = synthetic_app();
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, true);
    app_set_screen(&mut app, AppScreen::Playing);

    let prefs_before = app.world().resource::<GameConfig>().prefs;
    let init_music = prefs_before.music_on;
    let init_sfx = prefs_before.sfx_on;
    let init_timer = prefs_before.timer_on;
    let init_text = prefs_before.text_msg_on;

    step_app(&mut app, &AppInput {
        f7: true,
        f8: true,
        key_t: true,
        key_m: true,
        ..Default::default()
    });

    let prefs_after = app.world().resource::<GameConfig>().prefs;
    assert_ne!(prefs_after.music_on, init_music);
    assert_ne!(prefs_after.sfx_on, init_sfx);
    assert_ne!(prefs_after.timer_on, init_timer);
    assert_ne!(prefs_after.text_msg_on, init_text);
    let meta = app_meta(&app);
    assert_eq!(meta.timer_enabled, prefs_after.timer_on != 0);
    assert_eq!(meta.text_enabled, prefs_after.text_msg_on != 0);
}

#[test]
fn f1_during_solved_animation_does_not_play_hint_cue() {
    let mut app = synthetic_app();
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, false);
    app_set_screen(&mut app, AppScreen::Playing);
    app_auto_solve(&mut app);
    assert!(app_meta(&app).solved);
    assert!(app_active_anim(&mut app).is_some());
    assert!(app.world().non_send::<AudioEngine>().common_caption().is_none());

    step_app(&mut app, &AppInput {
        f1: true,
        ..Default::default()
    });

    assert!(app.world().non_send::<AudioEngine>().common_caption().is_none());
}

#[test]
fn escape_prompts_confirmation_when_all_placed_pieces_are_held_as_group() {
    let mut app = synthetic_app();
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 4, false, false, false);
    app_set_screen(&mut app, AppScreen::Playing);
    let pf_x = app_meta(&app).playfield_x();
    let pf_y = app_meta(&app).playfield_y();

    // Place 2 adjacent pieces (0, 1)=1 and (1, 1)=5, then right-click to pick up the entire cluster.
    for (pidx, r, c) in [(1usize, 0usize, 1usize), (5, 1, 1)] {
        let pos = app_tray_order(&mut app).iter().position(|&p| p == pidx).unwrap();
        app_meta_mut(&mut app).tray_scroll = pos;
        app_click(&mut app, 20, 40);
        let col_w = crate::puzzle::grid_col_widths(4);
        let row_h = crate::puzzle::grid_row_heights(4);
        let cx = pf_x + col_w[..c].iter().sum::<i32>() + col_w[c] / 2;
        let cy = pf_y + row_h[..r].iter().sum::<i32>() + row_h[r] / 2;
        app_click(&mut app, cx, cy);
    }
    let col_w = crate::puzzle::grid_col_widths(4);
    let row_h = crate::puzzle::grid_row_heights(4);
    let x01 = pf_x + col_w[0] + col_w[1] / 2;
    let y01 = pf_y + row_h[0] / 2;
    assert!(app_right_click(&mut app, x01, y01));
    assert_eq!(app_held_group_pieces(&mut app).len(), 2);
    assert!(!app_all_placed(&mut app));

    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });

    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(app.world().resource::<InGameMenuState>().confirm_exit_puzzle);
    assert!(
        app_meta(&app)
            .status_message()
            .is_some_and(|m| m.contains("We're not done yet"))
    );
}

#[test]
fn browser_quit_and_print_buttons_and_hotkeys() {
    let mut app = synthetic_app();
    app_set_screen(&mut app, AppScreen::Browser);
    assert!(!app.world_mut().resource_mut::<ScreenState>().quit_requested);

    // Clicking Print button (index 6) sets browser_status.
    let print_btn = crate::app::BROWSER_BUTTONS[6];
    assert_eq!(print_btn.action, crate::app::BrowserAction::PrintPuzzle);
    step_app(&mut app, &AppInput {
        left_pressed: true,
        pointer_x: print_btn.x + print_btn.w / 2,
        pointer_y: print_btn.y + print_btn.h / 2,
        ..Default::default()
    });
    assert!(app.world().resource::<BrowserState>().status.is_some());

    // Pressing P triggers PrintPuzzle as well.
    app.world_mut().resource_mut::<BrowserState>().status = None;
    step_app(&mut app, &AppInput {
        key_p: true,
        ..Default::default()
    });
    assert!(app.world().resource::<BrowserState>().status.is_some());

    // Clicking Quit button (index 7) requests quit.
    let quit_btn = crate::app::BROWSER_BUTTONS[7];
    assert_eq!(quit_btn.action, crate::app::BrowserAction::QuitGame);
    step_app(&mut app, &AppInput {
        left_pressed: true,
        pointer_x: quit_btn.x + quit_btn.w / 2,
        pointer_y: quit_btn.y + quit_btn.h / 2,
        ..Default::default()
    });
    assert!(app.world_mut().resource_mut::<ScreenState>().quit_requested);

    // Pressing Q or Escape in Browser also sets quit_requested.
    app.world_mut().resource_mut::<ScreenState>().quit_requested = false;
    step_app(&mut app, &AppInput {
        key_q: true,
        ..Default::default()
    });
    assert!(app.world_mut().resource_mut::<ScreenState>().quit_requested);

    app.world_mut().resource_mut::<ScreenState>().quit_requested = false;
    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(app.world_mut().resource_mut::<ScreenState>().quit_requested);
}

#[test]
fn mickey_cursor_overlay_draws_without_invalidating_base_cache() {
    let mut app = synthetic_app();
    app_set_screen(&mut app, AppScreen::Browser);

    // Populate synthetic sprites up to index 64 (62=pointing hand, 63=grab hand, 64=open hand)
    // with a solid white pixel at (0,0). Hotspot for 62 is (-6, 0).
    let mut palette = [[0u8; 3]; 256];
    palette[1] = [255, 250, 240];
    {
        let tbx = &mut app.world_mut().resource_mut::<StaticAssets>().tbx;
        tbx.palette = palette;
        while tbx.sprites.len() <= 64 {
            tbx.sprites.push(crate::assets::Sprite8061::from_8bpp(
                2,
                2,
                0,
                vec![1, 0, 0, 0],
                vec![true, false, false, false],
            ));
        }
    }
    assert!(has_mickey_cursor(&app));

    let mut renderer = AppRenderer::new();
    assert!(renderer.render_if_needed(app.world_mut(), 100, 80, false, false));
    let off_100 = (80 * 320 + (100 - 6)) * 4;
    assert_eq!(
        &renderer.rgba_buf[off_100..off_100 + 4],
        &[255, 250, 240, 255]
    );

    // Same pointer and state -> no re-render needed.
    assert!(!renderer.render_if_needed(app.world_mut(), 100, 80, false, false));

    // Moving pointer updates cursor pixel at (140 - 6, 90) and restores (94, 80) from base cache.
    assert!(renderer.render_if_needed(app.world_mut(), 140, 90, false, false));
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
    {
        let tbx = &mut app.world_mut().resource_mut::<StaticAssets>().tbx;
        tbx.palette = palette;
        while tbx.sprites.len() <= 64 {
            tbx.sprites.push(crate::assets::Sprite8061::from_8bpp(
                2,
                2,
                0,
                vec![1, 0, 0, 0],
                vec![true, false, false, false],
            ));
        }
    }

    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 4, false, false, false);
    app_set_screen(&mut app, AppScreen::Playing);

    // 1. WASD steers keyboard_pointer away from physical mouse coordinates (150, 120).
    step_app(&mut app, &AppInput {
        dt: 0.25,
        move_x: 1,
        pointer_x: 150,
        pointer_y: 120,
        ..Default::default()
    });
    let kbd_pos = app_meta(&app).keyboard_pointer.unwrap();
    assert_eq!(
        app_meta(&app).active_pointer(150, 120),
        (kbd_pos.0.round() as i32, kbd_pos.1.round() as i32)
    );

    // 2. Physical mouse motion without clicking clears keyboard_pointer so Mickey hand tracks (150, 120).
    step_app(&mut app, &AppInput {
        pointer_x: 150,
        pointer_y: 120,
        pointer_moved: true,
        ..Default::default()
    });
    assert!(app_meta(&app).keyboard_pointer.is_none());
    assert_eq!(app_meta(&app).active_pointer(150, 120), (150, 120));

    let mut renderer = AppRenderer::new();
    assert!(renderer.render_if_needed(app.world_mut(), 150, 120, false, false));
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
    app_set_screen(&mut app, AppScreen::Browser);

    // Without active audio sinks, Quit exits immediately.
    step_app(&mut app, &AppInput {
        key_q: true,
        ..Default::default()
    });
    assert!(app.world().resource::<ScreenState>().quit_requested);
    assert!(should_exit(&app));

    // With an active Rodio sink, Quit enters a noninteractive wait until playback ends or times out.
    app.world_mut().resource_mut::<ScreenState>().quit_requested = false;
    app.world_mut().resource_mut::<ScreenState>().quit_timer.reset();
    let (sink, _queue) = rodio::Sink::new_idle();
    sink.append(rodio::buffer::SamplesBuffer::new(
        1,
        11_025,
        vec![100i16; 4096],
    ));
    app.world_mut().non_send_mut::<AudioEngine>().sfx_sinks.push(sink);

    step_app(&mut app, &AppInput {
        key_q: true,
        ..Default::default()
    });
    assert!(app.world().resource::<ScreenState>().quit_requested);
    assert!(!should_exit(&app));

    // Interactive keys during quitting phase are ignored, while quit_elapsed advances.
    step_app(&mut app, &AppInput {
        dt: 0.5,
        key_p: true,
        ..Default::default()
    });
    assert!(app.world().resource::<BrowserState>().status.is_none());
    assert!(!should_exit(&app));

    // Bounded fallback exits cleanly once elapsed >= 3.0s even if the sink hasn't drained.
    step_app(&mut app, &AppInput {
        dt: 2.6,
        ..Default::default()
    });
    assert!(should_exit(&app));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn browser_renders_original_tbx_sprites_in_vga_and_ega() {
    for ega in [false, true] {
        let mut app = test_browser();
        app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = if ega { 2 } else { 1 };
        app_sync_prefs_from_setup(&mut app);
        let mut renderer = AppRenderer::new();
        {
            let browser = &mut app.world_mut().resource_mut::<BrowserState>();
            browser.hover = None;
            browser.pressed = None;
        }
        renderer.render(app.world_mut(), 160, 100);
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
        let preview_asset = app.world().resource::<BrowserState>().preview_asset.clone();
        crate::graphics::render_scene_view_with_scratch(
            &mut centered,
            &mut native,
            &preview_asset,
            preview_asset.static_frame_idx(),
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
    app_set_screen(&mut app, AppScreen::Browser);
    {
        let preview_asset = &mut app.world_mut().resource_mut::<BrowserState>().preview_asset;
        while preview_asset.sequences.len() < 2 {
            preview_asset.sequences.push(NamedSequence {
                name: "STILL".to_string(),
                header_idx: 0,
                start_frame: 0,
                end_frame: 0,
            });
        }
        preview_asset.sequences[1].start_frame = 0;
        preview_asset.palette[3] = [255, 0, 0];
        preview_asset.palette[4] = [0, 255, 0];
        let spr = &mut preview_asset.sprites[0];
        for y in 0..200 {
            let row = y * (spr.stride as usize);
            for x in 2..6 {
                spr.pixels[row + x] = 3;
            }
            for x in 268..272 {
                spr.pixels[row + x] = 4;
            }
        }
    }

    let mut renderer = AppRenderer::new();
    renderer.render(app.world_mut(), 160, 100);
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
    assert!(renderer.render_if_needed(app.world_mut(), 160, 80, false, false));

    // Hovering over NextPuzzle changes browser_hover and triggers a re-render.
    let next_btn = BROWSER_BUTTONS[1];
    assert_eq!(next_btn.action, crate::app::BrowserAction::NextPuzzle);
    step_app(&mut app, &AppInput {
        pointer_x: next_btn.x + next_btn.w / 2,
        pointer_y: next_btn.y + next_btn.h / 2,
        pointer_moved: true,
        ..Default::default()
    });
    assert_eq!(
        app.world().resource::<BrowserState>().hover,
        Some(crate::app::BrowserTarget::Button(
            crate::app::BrowserAction::NextPuzzle
        ))
    );
    assert!(renderer.render_if_needed(
        app.world_mut(),
        next_btn.x + next_btn.w / 2,
        next_btn.y + next_btn.h / 2,
        false,
        false
    ));

    // Clicking right-sidebar slot 5 selects puzzle 5 and sets browser_pressed until release.
    let slot5_y = crate::app::BROWSER_SLOT_Y0 + 4 * crate::app::BROWSER_SLOT_H + 5;
    step_app(&mut app, &AppInput {
        pointer_x: 312,
        pointer_y: slot5_y,
        left_pressed: true,
        ..Default::default()
    });
    assert_eq!(app.world().resource::<BrowserState>().selected_puzzle, 5);
    assert_eq!(
        app.world().resource::<BrowserState>().pressed,
        Some(crate::app::BrowserTarget::PuzzleSlot(5))
    );
    step_app(&mut app, &AppInput {
        pointer_x: 312,
        pointer_y: slot5_y,
        left_released: true,
        ..Default::default()
    });
    assert!(app.world().resource::<BrowserState>().pressed.is_none());
}

#[test]
fn mickey_cursor_uses_correct_sprites_hotspots_and_viewport_bounds() {
    let mut app = synthetic_app();
    let mut palette = [[0u8; 3]; 256];
    palette[62] = [255, 0, 0];
    palette[63] = [0, 255, 0];
    palette[64] = [0, 0, 255];
    {
        let tbx = &mut app.world_mut().resource_mut::<StaticAssets>().tbx;
        tbx.palette = palette;
        for idx in 0..=64u8 {
            let color = if idx >= 62 { idx } else { 1 };
            tbx.sprites.push(crate::assets::Sprite8061::from_8bpp(
                1,
                1,
                0,
                vec![color],
                vec![true],
            ));
        }
    }

    let mut renderer = AppRenderer::new();

    // Browser idle -> sprite 62 (red), hotspot (-6, 0).
    app_set_screen(&mut app, AppScreen::Browser);
    renderer.render_if_needed(app.world_mut(), 100, 80, false, false);
    let off_62 = (80 * 320 + (100 - 6)) * 4;
    assert_eq!(&renderer.rgba_buf[off_62..off_62 + 4], &[255, 0, 0, 255]);

    // Browser pressed -> sprite 63 (green), hotspot (-10, 0).
    app.world_mut().resource_mut::<BrowserState>().pressed =
        Some(crate::app::BrowserTarget::PuzzleSlot(1));
    renderer.render_if_needed(app.world_mut(), 100, 80, false, false);
    let off_63 = (80 * 320 + (100 - 10)) * 4;
    assert_eq!(&renderer.rgba_buf[off_63..off_63 + 4], &[0, 255, 0, 255]);
    app.world_mut().resource_mut::<BrowserState>().pressed = None;

    // Playing idle -> sprite 64 (blue), hotspot (-7, 0).
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, false, false, false);
    app_set_screen(&mut app, AppScreen::Playing);
    assert_eq!(app_screen(&app), AppScreen::Playing);
    renderer.render_if_needed(app.world_mut(), 100, 80, false, false);
    let off_64 = (80 * 320 + (100 - 7)) * 4;
    assert_eq!(&renderer.rgba_buf[off_64..off_64 + 4], &[0, 0, 255, 255]);

    // Playing over open toolbox (px < 46) -> sprite 62 (red), hotspot (-6, 0).
    app_meta_mut(&mut app).toolbox_open = true;
    renderer.render_if_needed(app.world_mut(), 20, 80, false, false);
    let off_62_tbx = (80 * 320 + (20 - 6)) * 4;
    assert_eq!(
        &renderer.rgba_buf[off_62_tbx..off_62_tbx + 4],
        &[255, 0, 0, 255]
    );
    app_meta_mut(&mut app).toolbox_open = false;

    // Out-of-viewport pointer -> cursor is not drawn and stays cached as None.
    assert!(renderer.render_if_needed(app.world_mut(), -1, 80, false, false));
    assert!(!renderer.render_if_needed(app.world_mut(), 320, 80, false, false));
    assert_ne!(&renderer.rgba_buf[off_64..off_64 + 4], &[0, 0, 255, 255]);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn toolbox_open_closed_hover_pressed_and_cursor_states() {
    let mut app = test_browser();
    let asset = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, asset, 3, true, false, false);
    app_meta_mut(&mut app).toolbox_open = true;
    app_set_screen(&mut app, AppScreen::Playing);

    let mut renderer = AppRenderer::new();

    // Open toolbox: x=0..2 stays black, tbx[49] icon renders inside Menu button at (3, 2).
    renderer.render_if_needed(app.world_mut(), 200, 100, false, false);
    let black_idx = renderer.fb.color_index([0, 0, 0]);
    assert_eq!(renderer.fb.get_pixel(1, 100), black_idx);
    assert_eq!(
        AppRenderer::desired_cursor(app.world_mut(), 200, 100, false),
        Some((64, 193, 100)),
        "playfield idle uses cursor 64"
    );
    let menu_baseline = renderer.fb.pixels;

    {
        let tbx = &app.world().resource::<StaticAssets>().tbx;
        let icon_49 = &tbx.sprites[49];
        let mut checked_icon_px = false;
        for dy in 0..icon_49.sub_h as i32 {
            for dx in 0..icon_49.width as i32 {
                let sy = icon_49.sub_y as usize + dy as usize;
                let off = sy * icon_49.stride as usize + icon_49.sub_x as usize + dx as usize;
                if icon_49.is_opaque(off) {
                    let expected = renderer
                        .fb
                        .color_index(tbx.palette[icon_49.pixels[off] as usize]);
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
    }

    // Hovering Menu, ScrollUp (inactive at 0), ScrollDown, and TraySlot(0).
    renderer.render_if_needed(app.world_mut(), 20, 8, false, false);
    assert_ne!(renderer.fb.pixels, menu_baseline, "Menu hover must repaint");
    assert_eq!(
        AppRenderer::desired_cursor(app.world_mut(), 20, 8, false),
        Some((62, 14, 8))
    );

    renderer.render_if_needed(app.world_mut(), 20, 192, false, false);
    assert_ne!(
        renderer.fb.pixels, menu_baseline,
        "ScrollDown hover must repaint"
    );
    let hover_scroll_down = renderer.fb.pixels;

    // Pressing ScrollDown switches visual to pressed state and cursor to 63, then scrolls on release.
    assert_eq!(
        app_meta(&app).toolbox_target_at(20, 192, false, 9),
        Some(crate::puzzle::ToolboxTarget::ScrollDown)
    );
    app_press(&mut app, 20, 192);
    assert_eq!(
        app_meta(&app).pressed_toolbox_target,
        Some(crate::puzzle::ToolboxTarget::ScrollDown)
    );
    renderer.render_if_needed(app.world_mut(), 20, 192, false, false);
    assert_ne!(
        renderer.fb.pixels, hover_scroll_down,
        "ScrollDown press must repaint"
    );
    assert_eq!(
        AppRenderer::desired_cursor(app.world_mut(), 20, 192, false),
        Some((63, 10, 192))
    );

    app_release(&mut app, 20, 192);
    assert!(app_meta(&app).pressed_toolbox_target.is_none());
    assert_eq!(app_meta(&app).tray_scroll, 1);

    // Collapsed toolbox: tab at (2, 2) renders tbx[49], responds to hover/press/release.
    { let open = app_meta(&app).toolbox_open; app_meta_mut(&mut app).toolbox_open = !open; }
    assert!(!app_meta(&app).toolbox_open);
    renderer.render_if_needed(app.world_mut(), 200, 100, false, false);
    let closed_idle = renderer.fb.pixels;
    assert_eq!(
        app_meta(&app).toolbox_target_at(8, 8, false, 9),
        Some(crate::puzzle::ToolboxTarget::ClosedTab)
    );
    assert_eq!(app_meta(&app).toolbox_target_at(20, 8, false, 9), None);
    assert_eq!(
        AppRenderer::desired_cursor(app.world_mut(), 8, 8, false),
        Some((62, 2, 8))
    );

    renderer.render_if_needed(app.world_mut(), 8, 8, false, false);
    assert_ne!(
        renderer.fb.pixels, closed_idle,
        "ClosedTab hover must repaint"
    );

    app_press(&mut app, 8, 8);
    assert_eq!(
        AppRenderer::desired_cursor(app.world_mut(), 8, 8, false),
        Some((63, -2, 8))
    );
    renderer.render_if_needed(app.world_mut(), 8, 8, false, false);
    app_release(&mut app, 8, 8);
    assert!(app_meta(&app).toolbox_open);

    // ClosedTab horizontal edges (x = 0, 16, 17 open; x = 18 does not) and while holding a piece.
    for edge_x in [0, 16, 17] {
        app_meta_mut(&mut app).toolbox_open = false;
        assert_eq!(
            app_meta(&app).toolbox_target_at(edge_x, 8, false, 9),
            Some(crate::puzzle::ToolboxTarget::ClosedTab)
        );
        app_press(&mut app, edge_x, 8);
        app_release(&mut app, edge_x, 8);
        assert!(
            app_meta(&app).toolbox_open,
            "ClosedTab edge x={edge_x} must open toolbox"
        );
    }
    app_meta_mut(&mut app).toolbox_open = false;
    assert_eq!(app_meta(&app).toolbox_target_at(18, 8, false, 9), None);
    app_press(&mut app, 18, 8);
    app_release(&mut app, 18, 8);
    assert!(!app_meta(&app).toolbox_open, "x=18 is outside ClosedTab");

    // Holding a piece while clicking rightmost tbx[49] column (x=17) still opens toolbox.
    app_set_piece_held(&mut app, 0);
    app_press(&mut app, 17, 8);
    app_release(&mut app, 17, 8);
    assert!(app_meta(&app).toolbox_open);
    assert_eq!(app_held_piece(&mut app), Some(0));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn toolbox_tray_slots_scrolling_removal_and_return() {
    let mut app = test_browser();
    let asset = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, asset, 3, true, false, false);
    let vis = app_meta(&app).visible_tray_slots();
    let slot_h = app_meta(&app).tray_slot_height();
    assert_eq!(vis, 3);

    // Every visible slot (first through last) is hit-tested accurately; non-slot regions return None.
    assert_eq!(
        app_meta(&app).toolbox_target_at(20, 8, false, 9),
        Some(crate::puzzle::ToolboxTarget::Menu)
    );
    assert_eq!(
        app_meta(&app).toolbox_target_at(20, 22, false, 9),
        Some(crate::puzzle::ToolboxTarget::ScrollUp)
    );
    assert_eq!(
        app_meta(&app).toolbox_target_at(20, 192, false, 9),
        Some(crate::puzzle::ToolboxTarget::ScrollDown)
    );
    assert!(!matches!(app_meta(&app).toolbox_target_at(20, 8, false, 9), Some(crate::puzzle::ToolboxTarget::TraySlot(_))));
    assert!(!matches!(app_meta(&app).toolbox_target_at(20, 22, false, 9), Some(crate::puzzle::ToolboxTarget::TraySlot(_))));
    assert!(!matches!(app_meta(&app).toolbox_target_at(20, 192, false, 9), Some(crate::puzzle::ToolboxTarget::TraySlot(_))));
    assert_eq!(app_meta(&app).toolbox_target_at(48, 50, false, 9), None);
    for slot in 0..vis {
        let y = 30 + slot as i32 * slot_h + 2;
        assert_eq!(app_meta(&app).toolbox_target_at(20, y, false, 9), Some(crate::puzzle::ToolboxTarget::TraySlot(slot)));
    }

    // Scroll boundaries, non-interrupting UI click audio, & tray removal + return.
    app.world_mut().non_send_mut::<AudioEngine>().play_common(crate::audio::CommonSound::ChoosePiece);
    let caption_before = app.world().non_send::<AudioEngine>().caption_key();
    app_set_screen(&mut app, AppScreen::Playing);
    app_click(&mut app, 20, 22);
    assert_eq!(app_meta(&app).tray_scroll, 0);
    app_click(&mut app, 20, 192);
    assert_eq!(app_meta(&app).tray_scroll, 1);
    app_click(&mut app, 20, 192);
        assert_eq!(app_meta(&app).tray_scroll, 2);
    assert_eq!(
        app.world().non_send::<AudioEngine>().caption_key(),
        caption_before,
        "tray scroll UI click must not clear active speech caption"
    );
    app_click(&mut app, 20, 22);
        assert_eq!(app_meta(&app).tray_scroll, 1);

    let expected_first_after_scroll = app_tray_order(&mut app)[1];
    app_click(&mut app, 20, 32);
    assert_eq!(app_held_piece(&mut app), Some(expected_first_after_scroll));
    assert_eq!(app_tray_order(&mut app).len(), 8);

    // Returning piece to slot 0 restores it at tray_scroll.
    app_click(&mut app, 20, 32);
    assert!(app_held_piece(&mut app).is_none());
    assert_eq!(app_tray_order(&mut app).len(), 9);
    assert_eq!(app_tray_order(&mut app)[1], expected_first_after_scroll);

    // Picking pieces from the last page until tray_scroll >= tray_order.len() clamps scroll so tray is not empty.
    let total = app_tray_order(&mut app).len();
    app_meta_mut(&mut app).tray_scroll = total - vis;
    for _ in 0..vis {
        let y = 30 + (vis as i32 - 1) * slot_h + 2;
        app_click(&mut app, 20, y);
        if let Some(p) = app_held_piece(&mut app) {
            app_set_piece_placed(&mut app, p, true);
        }
    }
    assert!(app_meta(&app).tray_scroll < app_tray_order(&mut app).len());
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn connected_group_pickup_render_movement_and_tray_return() {
    let mut app = test_browser();
    let asset = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, asset, 2, true, false, false);
    app_set_screen(&mut app, AppScreen::Playing);
    let col_w = crate::puzzle::grid_col_widths(2);
    let row_h = crate::puzzle::grid_row_heights(2);
    let pf_x = app_meta(&app).playfield_x();
    let pf_y = app_meta(&app).playfield_y();
    let p0 = 0usize;
    let p1 = 1usize;
    for (pidx, r, c) in [(p0, 0usize, 0usize), (p1, 0, 1)] {
        let pos = app_tray_order(&mut app).iter().position(|&p| p == pidx).unwrap();
        app_meta_mut(&mut app).tray_scroll = pos;
        app_click(&mut app, 20, 40);
        let cx = pf_x + col_w[..c].iter().sum::<i32>() + col_w[c] / 2;
        let cy = pf_y + row_h[..r].iter().sum::<i32>() + row_h[r] / 2;
        app_click(&mut app, cx, cy);
    }

    let c0x = pf_x + col_w[0] / 2;
    let c0y = pf_y + row_h[0] / 2;
    assert!(app_right_click(&mut app, c0x, c0y));
    assert_eq!(app_held_piece(&mut app), Some(p0));
    assert_eq!(app_held_group_pieces(&mut app).len(), 2);

    let mut renderer = AppRenderer::new();

    renderer.render_if_needed(app.world_mut(), 150, 90, false, false);
    let frame_pos1 = renderer.fb.pixels;
    renderer.render_if_needed(app.world_mut(), 200, 110, false, false);
    assert_ne!(
        renderer.fb.pixels, frame_pos1,
        "moving held connected group must update rendered playfield"
    );

    // Return the entire connected group to the tray.
    app_click(&mut app, 20, 40);
    assert!(app_held_piece(&mut app).is_none());
    assert!(app_held_group_pieces(&mut app).is_empty());
    let order = app_tray_order(&mut app);
    assert!(order.contains(&p0));
    assert!(order.contains(&p1));
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn vga_ega_switch_during_unfinished_puzzle_remaps_toolbox_and_border() {
    let mut app = test_browser();
    app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = 1; // VGA
    app_sync_prefs_from_setup(&mut app);
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, false);
    app_set_screen(&mut app, AppScreen::Playing);
    let pos = app_tray_order(&mut app).iter().position(|&p| p == 0).unwrap();
    app_meta_mut(&mut app).tray_scroll = pos;
    app_click(&mut app, 20, 40);
    let piece0 = app_piece_geom(&mut app, 0);
    let cx = app_meta(&app).playfield_x() + piece0.base_x + piece0.base_w / 2;
    let cy = app_meta(&app).playfield_y() + piece0.base_y + piece0.base_h / 2;
    app_click(&mut app, cx, cy);

    let mut renderer = AppRenderer::new();
    renderer.render_if_needed(app.world_mut(), 200, 100, false, false);
    let vga_pixels = renderer.fb.pixels;
    let vga_asset_rev = app_meta(&app).asset_rev;

    // Switch to EGA while puzzle is unfinished.
    app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = 2;
    app_sync_prefs_from_setup(&mut app);
    assert!(app.world().resource::<SetupState>().menu.error.is_none());
    assert_ne!(app_meta(&app).asset_rev, vga_asset_rev);
    assert!(app_is_placed(&mut app, 0));

    renderer.render_if_needed(app.world_mut(), 200, 100, false, false);
    assert_ne!(
        renderer.fb.pixels, vga_pixels,
        "EGA switch must invalidate toolbox & board caches"
    );

    // Verify tbx_ega[49] in open Menu is remapped to the EGA puzzle palette.
    let icon_49 = &app.world().resource::<StaticAssets>().tbx_ega.sprites[49];
    for dy in 0..icon_49.sub_h as i32 {
        for dx in 0..icon_49.width as i32 {
            let sy = icon_49.sub_y as usize + dy as usize;
            let off = sy * icon_49.stride as usize + icon_49.sub_x as usize + dx as usize;
            if icon_49.is_opaque(off) {
                let expected = renderer
                    .fb
                    .color_index(app.world().resource::<StaticAssets>().tbx_ega.palette[icon_49.pixels[off] as usize]);
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
    {
        let tbx = &mut app.world_mut().resource_mut::<StaticAssets>().tbx;
        while tbx.sprites.len() <= 64 {
            tbx.sprites.push(crate::assets::Sprite8061::from_8bpp(
                16,
                16,
                0,
                vec![1; 256],
                vec![true; 256],
            ));
        }
    }
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, true);
    app_set_screen(&mut app, AppScreen::Playing);

    // Click MENU button in open toolbox (20, 8) -> press + release opens menu.
    step_app(&mut app, &AppInput {
        pointer_x: 20,
        pointer_y: 8,
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.world().resource::<InGameMenuState>().open);
    step_app(&mut app, &AppInput {
        pointer_x: 20,
        pointer_y: 8,
        left_released: true,
        ..Default::default()
    });
    assert!(app.world().resource::<InGameMenuState>().open);
    assert_eq!(app.world().resource::<InGameMenuState>().selected, 0);

    // Cursor is pointing hand (62) over menu; present_signature tracks hover/press.
    let mut renderer = AppRenderer::new();
    renderer.render_if_needed(app.world_mut(), 120, 54, false, false);
    assert_eq!(
        AppRenderer::desired_cursor(app.world_mut(), 120, 54, false),
        Some((62, 114, 54))
    );
    let sig_hover0 = present_signature(app.world_mut(), 120, 54, false, false);
    let sig_hover1 = present_signature(app.world_mut(), 120, 74, false, false);
    assert_ne!(sig_hover0, sig_hover1);

    // Down skips disabled Replay (when unsolved) and wraps cleanly.
    // Enabled items when unsolved: 0 OtherPicture, 1 Setup, 2 PieceTray, 4 Quit, 5 Resume.
    for expected in [1, 2, 4, 5, 0] {
        step_app(&mut app, &AppInput {
            down: true,
            ..Default::default()
        });
        assert_eq!(app.world().resource::<InGameMenuState>().selected, expected);
    }
    step_app(&mut app, &AppInput {
        up: true,
        ..Default::default()
    });
    assert_eq!(app.world().resource::<InGameMenuState>().selected, 5);

    // Press on Resume, drag off before release -> does NOT close menu.
    let (rx, ry, _, _) = crate::app::in_game_menu_row_rect(5);
    step_app(&mut app, &AppInput {
        pointer_x: rx + 10,
        pointer_y: ry + 5,
        left_pressed: true,
        ..Default::default()
    });
    assert_eq!(
        app.world().resource::<InGameMenuState>().pressed,
        Some(crate::app::InGameMenuCommand::Resume)
    );
    assert_eq!(
        AppRenderer::desired_cursor(app.world_mut(), rx + 10, ry + 5, false),
        Some((63, rx, ry + 5))
    );
    step_app(&mut app, &AppInput {
        pointer_x: rx + 10,
        pointer_y: ry + 40,
        left_released: true,
        ..Default::default()
    });
    assert!(app.world().resource::<InGameMenuState>().open);
    assert!(app.world().resource::<InGameMenuState>().pressed.is_none());

    // Clicking outside menu panel closes it cleanly.
    step_app(&mut app, &AppInput {
        pointer_x: 10,
        pointer_y: 10,
        left_pressed: true,
        ..Default::default()
    });
    assert!(!app.world().resource::<InGameMenuState>().open);

    // Re-open and close with Escape; keyboard-to-mouse switching clears keyboard pointer.
    app_open_in_game_menu(&mut app);
    step_app(&mut app, &AppInput {
        dt: 0.1,
        move_x: 1,
        ..Default::default()
    });
    assert!(app_meta(&app).keyboard_pointer.is_some());
    step_app(&mut app, &AppInput {
        pointer_x: 120,
        pointer_y: 54,
        pointer_moved: true,
        ..Default::default()
    });
    assert!(app_meta(&app).keyboard_pointer.is_none());
    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(!app.world().resource::<InGameMenuState>().open);

    // Stationary pointer over disabled Replay (row 3) or OtherPicture (row 0)
    // must not override keyboard Down + Enter selection of Setup (row 1).
    app_open_in_game_menu(&mut app);
    let (r0x, r0y, _, _) = crate::app::in_game_menu_row_rect(0);
    let (r3x, r3y, _, _) = crate::app::in_game_menu_row_rect(3);
    step_app(&mut app, &AppInput {
        pointer_x: r3x + 10,
        pointer_y: r3y + 5,
        pointer_moved: true,
        ..Default::default()
    });
    assert_eq!(app.world().resource::<InGameMenuState>().selected, 0);
    step_app(&mut app, &AppInput {
        pointer_x: r0x + 10,
        pointer_y: r0y + 5,
        down: true,
        ..Default::default()
    });
    assert_eq!(app.world().resource::<InGameMenuState>().selected, 1);
    step_app(&mut app, &AppInput {
        pointer_x: r0x + 10,
        pointer_y: r0y + 5,
        enter: true,
        ..Default::default()
    });
    assert!(matches!(app_screen(&app), AppScreen::Setup { .. }));
    assert!(!app.world().resource::<InGameMenuState>().open);
    app_exit_setup(&mut app, true);

    // WASD then physical mouse click *without* pointer_moved clears keyboard pointer
    // and hits physical mouse coordinates instead of stale keyboard pointer.
    app_open_in_game_menu(&mut app);
    step_app(&mut app, &AppInput {
        dt: 0.1,
        move_x: 1,
        ..Default::default()
    });
    assert!(app_meta(&app).keyboard_pointer.is_some());
    let (r1x, r1y, _, _) = crate::app::in_game_menu_row_rect(1);
    step_app(&mut app, &AppInput {
        pointer_x: r1x + 10,
        pointer_y: r1y + 5,
        left_pressed: true,
        ..Default::default()
    });
    assert!(app_meta(&app).keyboard_pointer.is_none());
    step_app(&mut app, &AppInput {
        pointer_x: r1x + 10,
        pointer_y: r1y + 5,
        left_released: true,
        ..Default::default()
    });
    assert!(matches!(app_screen(&app), AppScreen::Setup { .. }));
    app_exit_setup(&mut app, true);

    // Menu UI clicks (open, Up/Down, empty-handed PieceTray) must not interrupt active captions/speech.
    app.world_mut().non_send_mut::<AudioEngine>().play_common(crate::audio::CommonSound::NotDoneYet);
    { let mut s = AudioStatus::default(); sync_audio_status(app.world().non_send::<AudioEngine>(), &mut s); *app.world_mut().resource_mut::<AudioStatus>() = s; }
    let active_caption = app.world().non_send::<AudioEngine>().common_caption();
    assert!(active_caption.is_some());
    app_open_in_game_menu(&mut app);
        assert_eq!(app.world().non_send::<AudioEngine>().common_caption(), active_caption);
    step_app(&mut app, &AppInput {
        down: true,
        ..Default::default()
    });
    assert_eq!(app.world().non_send::<AudioEngine>().common_caption(), active_caption);
    app_exec_in_game_menu(&mut app, crate::app::InGameMenuCommand::PieceTray,
    );
        assert_eq!(app.world().non_send::<AudioEngine>().common_caption(), active_caption);
}

#[test]
fn in_game_menu_commands_other_picture_setup_tray_replay_and_quit() {
    let mut app = synthetic_app();
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, true);
    // Pick up piece 0 from tray and place piece 1 on board to create unfinished state + held piece.
    let last_tray = app_tray_order(&mut app).pop().unwrap();
    app_set_piece_held(&mut app, last_tray);
    app_set_piece_placed(&mut app, 1, true);
    app_meta_mut(&mut app).elapsed_secs = 42.5;
    let held_id = app_held_piece(&mut app);
    app_set_screen(&mut app, AppScreen::Playing);

    // 1. Setup from menu preserves active puzzle, held piece, and timer.
    app_open_in_game_menu(&mut app);
    let (sx, sy, _, _) = crate::app::in_game_menu_row_rect(1);
    step_app(&mut app, &AppInput {
        pointer_x: sx + 8,
        pointer_y: sy + 6,
        left_pressed: true,
        ..Default::default()
    });
    step_app(&mut app, &AppInput {
        pointer_x: sx + 8,
        pointer_y: sy + 6,
        left_released: true,
        ..Default::default()
    });
    assert_eq!(
        app_screen(&app),
        AppScreen::Setup {
            return_to_playing: true
        }
    );
    app_exit_setup(&mut app, true);
    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(!app.world().resource::<InGameMenuState>().open);
    assert_eq!(app_held_piece(&mut app), held_id);
    assert!(app_is_placed(&mut app, 1));
    assert!((app_meta(&app).elapsed_secs - 42.5).abs() < 1e-4);

    // 2. Replay is disabled while unsolved (both menu click and R hotkey).
    app_open_in_game_menu(&mut app);
    let (rx, ry, _, _) = crate::app::in_game_menu_row_rect(3);
    step_app(&mut app, &AppInput {
        pointer_x: rx + 8,
        pointer_y: ry + 6,
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.world().resource::<InGameMenuState>().open);
    assert!(app_active_anim(&mut app).is_none());
    app_close_in_game_menu(&mut app, false);
    step_app(&mut app, &AppInput {
        key_r: true,
        ..Default::default()
    });
    assert!(app_active_anim(&mut app).is_none());

    // 3. PieceTray returns held single piece when holding one, preserves held connected groups,
    //    and toggles tray when empty-handed.
    let extra = app_tray_order(&mut app).pop().unwrap();
    {
        let mut q = app.world_mut().query::<(&PieceIdentity, &mut PieceLocation)>();
        for (ident, mut loc) in q.iter_mut(app.world_mut()) {
            if ident.index == extra {
                *loc = PieceLocation::Held {
                    is_anchor: false,
                    group_offset: (10, 0),
                    group_order: 1,
                };
            }
        }
    }
    let tray_snapshot_before_group = app_tray_order(&mut app);
    app_exec_in_game_menu(&mut app, crate::app::InGameMenuCommand::PieceTray,
    );
    assert_eq!(app_held_piece(&mut app), held_id);
    assert_eq!(
        app_held_group_pieces(&mut app),
        vec![(held_id.unwrap(), 0, 0), (extra, 10, 0)]
    );
    assert_eq!(app_tray_order(&mut app), tray_snapshot_before_group);
    // Restore single held piece for single-piece PieceTray test.
    {
        let next_order = app_tray_order(&mut app).len();
        let mut q = app.world_mut().query::<(&PieceIdentity, &mut PieceLocation)>();
        for (ident, mut loc) in q.iter_mut(app.world_mut()) {
            if ident.index == extra {
                *loc = PieceLocation::Tray { order: next_order };
            }
        }
    }
    app_exec_in_game_menu(&mut app, crate::app::InGameMenuCommand::PieceTray,
    );
    assert!(app_held_piece(&mut app).is_none());
    assert!(app_meta(&app).toolbox_open);
    assert_eq!(app_tray_order(&mut app).first().copied(), held_id);
    app_exec_in_game_menu(&mut app, crate::app::InGameMenuCommand::PieceTray,
    );
    assert!(!app_meta(&app).toolbox_open);
    app_exec_in_game_menu(&mut app, crate::app::InGameMenuCommand::PieceTray,
    );
    assert!(app_meta(&app).toolbox_open);

    // 4. Other Picture prompts unfinished confirmation; cancel preserves state, confirm returns to Browser.
    app_exec_in_game_menu(&mut app, crate::app::InGameMenuCommand::OtherPicture,
    );
        assert!(app.world().resource::<InGameMenuState>().confirm_exit_puzzle);
    assert_eq!(app.world().non_send::<AudioEngine>().common_caption(), Some("We're not done yet."));
    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(!app.world().resource::<InGameMenuState>().confirm_exit_puzzle);
    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(app_is_placed(&mut app, 1));

    // 5. Quit from unfinished puzzle prompts confirmation, cancel preserves state, confirm quits.
    step_app(&mut app, &AppInput {
        key_q: true,
        ..Default::default()
    });
    assert!(app.world().resource::<InGameMenuState>().confirm_quit_puzzle);
    assert!(!app.world().resource::<ScreenState>().quit_requested);
    assert_eq!(app.world().non_send::<AudioEngine>().common_caption(), Some("We're not done yet."));
    step_app(&mut app, &AppInput {
        escape: true,
        ..Default::default()
    });
    assert!(!app.world().resource::<InGameMenuState>().confirm_quit_puzzle);
    assert!(!app.world().resource::<ScreenState>().quit_requested);
    assert!(app_is_placed(&mut app, 1));

    app_exec_in_game_menu(&mut app, crate::app::InGameMenuCommand::Quit,
    );
    assert!(app.world().resource::<InGameMenuState>().confirm_quit_puzzle);
    step_app(&mut app, &AppInput {
        enter: true,
        ..Default::default()
    });
    assert!(!app.world().resource::<InGameMenuState>().confirm_quit_puzzle);
    assert!(app.world().resource::<ScreenState>().quit_requested);
}

#[test]
fn in_game_solved_replay_and_quit_behavior() {
    let mut app = synthetic_app();
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 2, true, false, true);
    app_set_screen(&mut app, AppScreen::Playing);
    app_auto_solve(&mut app);
    let anims: Vec<Entity> = { let w = app.world_mut(); w.query_filtered::<Entity, With<AnimationState>>().iter(w).collect() };
    for e in anims {
        app.world_mut().despawn(e);
    }

    // PieceTray is disabled when solved; Replay is enabled.
    assert!(!crate::ecs::systems::menu::is_menu_command_enabled(crate::app::InGameMenuCommand::PieceTray, app_meta(&app).solved));
    assert!(crate::ecs::systems::menu::is_menu_command_enabled(crate::app::InGameMenuCommand::Replay, app_meta(&app).solved));

    // Clicking top-left menu hotspot when solved opens the command menu.
    step_app(&mut app, &AppInput {
        pointer_x: 5,
        pointer_y: 5,
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.world().resource::<InGameMenuState>().open);

    // Replay starts animation and plays SeeWhatHappens ("Let's see what happens.").
    app_exec_in_game_menu(&mut app, crate::app::InGameMenuCommand::Replay,
    );
        assert!(!app.world().resource::<InGameMenuState>().open);
    assert!(app_active_anim(&mut app).is_some());
    assert_eq!(app.world().non_send::<AudioEngine>().common_caption(), Some("Let's see what happens."));

    // Repeated R hotkey restarts cleanly.
    step_app(&mut app, &AppInput {
        key_r: true,
        ..Default::default()
    });
    assert!(app_active_anim(&mut app).is_some());

    // Quit when solved asks confirmation WITHOUT playing "We're not done yet."
    app.world_mut().non_send_mut::<AudioEngine>().stop_all();
    { let mut s = AudioStatus::default(); sync_audio_status(app.world().non_send::<AudioEngine>(), &mut s); *app.world_mut().resource_mut::<AudioStatus>() = s; }
    step_app(&mut app, &AppInput {
        key_q: true,
        ..Default::default()
    });
    assert!(app.world().resource::<InGameMenuState>().confirm_quit_puzzle);
    assert_eq!(app.world().non_send::<AudioEngine>().common_caption(), None);
    step_app(&mut app, &AppInput {
        left_pressed: true,
        ..Default::default()
    });
    assert!(app.world().resource::<ScreenState>().quit_requested);
}

#[cfg_attr(
    not(feature = "asset-tests"),
    ignore = "requires original DOS game assets"
)]
#[test]
fn in_game_menu_renders_original_tbx_sprites_in_vga_and_ega() {
    let mut app = build_game_app(test_game_dir(), false).unwrap();
    app_set_screen(&mut app, AppScreen::Browser);
    app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 1;
    app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 2;
    app_start_selected_puzzle(&mut app);

    let mut renderer = AppRenderer::new();
    renderer.render_if_needed(app.world_mut(), 200, 100, false, false);
    let board_only = renderer.fb.pixels;

    // Idle board pointer motion keeps base cache clean while menu is closed.
    assert_eq!(
        present_signature(app.world_mut(), 200, 100, false, false),
        present_signature(app.world_mut(), 201, 101, false, false)
    );
    assert!(!renderer.render_if_needed(app.world_mut(), 200, 100, false, false));

    // Open menu in VGA and verify it renders over the board.
    app_open_in_game_menu(&mut app);
    assert!(renderer.render_if_needed(app.world_mut(), 120, 54, false, false));
    assert_ne!(renderer.fb.pixels, board_only);

    // Switch to EGA while menu is open -> Setup closes menu cleanly, then re-open in EGA.
    app_open_setup(&mut app, true);
    assert!(!app.world().resource::<InGameMenuState>().open);
    app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = 2;
    app_exit_setup(&mut app, true);
    app_open_in_game_menu(&mut app);
    assert!(renderer.render_if_needed(app.world_mut(), 120, 54, false, false));
}

fn composite_gpu_layers_sw(
    app: &mut App,
    app_renderer: &mut AppRenderer,
    mx: i32,
    my: i32,
    picture_guide: bool,
) -> Vec<u8> {
    use crate::graphics::SCREEN_H;

    let (px, py) = app_meta(app).active_pointer(mx, my);
    let uses_ega = app_meta(app).asset.uses_ega();
    let tbx = if uses_ega {
        app.world().resource::<StaticAssets>().tbx_ega.clone()
    } else {
        app.world().resource::<StaticAssets>().tbx.clone()
    };
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
        .render_board_layer_rgba(app.world_mut(), &tbx, picture_guide, &mut out);

    if let Some((_, rem)) = app_hint_piece(app) {
        let mut hint_buf = Vec::new();
        if let Some((hx, hy, hw, hh)) = app_renderer
            .puzzle_renderer
            .render_hint_layer_rgba(app.world_mut(), &mut hint_buf)
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
    let font = &app.world().resource::<StaticAssets>().font.clone();
    let (tx, ty, tw, th) = app_renderer.puzzle_renderer.render_toolbox_layer_rgba(
        app.world_mut(),
        &tbx,
        &font,
        px,
        py,
        &mut tbx_buf,
    );
    blit_layer(&mut out, &tbx_buf, tx, ty, tw, th);

    if let Some(pidx) = app_held_piece(app) {
        let mut held_buf = Vec::new();
        if let Some((rx, ry, hw, hh)) =
            app_renderer
                .puzzle_renderer
                .render_held_group_layer_rgba(app.world_mut(), px, py, &mut held_buf)
        {
            let anchor = app_piece_geom(app, pidx);
            let ax = px - anchor.base_w / 2;
            let ay = py - anchor.base_h / 2;
            blit_layer(&mut out, &held_buf, ax + rx, ay + ry, hw, hh);
        }
    }

    let mut overlay_buf = vec![0u8; (SCREEN_W * SCREEN_H * 4) as usize];
    if app_renderer.render_playing_overlay_rgba(app.world(), &mut overlay_buf) {
        blit_layer(&mut out, &overlay_buf, 0, 0, SCREEN_W, SCREEN_H);
    }

    if let Some((sprite_idx, cx, cy)) = AppRenderer::desired_cursor(app.world_mut(), mx, my, false) {
        let mut cur_buf = Vec::new();
        if let Some((cw, ch)) = AppRenderer::render_cursor_rgba(app.world(), sprite_idx, &mut cur_buf) {
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
        let mut app = build_game_app(test_game_dir(), false).unwrap();
        app.world_mut().resource_mut::<SetupState>().menu.prefs.video_mode = video_mode;
        app_exit_setup(&mut app, false);
        app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 1;
        app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 4;
        app_start_selected_puzzle(&mut app);

        let mut ref_renderer = AppRenderer::new();
        let mut layer_renderer = AppRenderer::new();

        // 1. Place two adjacent pieces (0 and 1), then pick up the connected group
        let col_w = crate::puzzle::grid_col_widths(4);
        let row_h = crate::puzzle::grid_row_heights(4);
        let pf_x = app_meta(&app).playfield_x();
        let pf_y = app_meta(&app).playfield_y();
        for (pidx, r, c) in [(0usize, 0usize, 0usize), (1usize, 0, 1)] {
            let pos = app_tray_order(&mut app).iter().position(|&p| p == pidx).unwrap();
            app_meta_mut(&mut app).tray_scroll = pos;
            app_click(&mut app, 20, 40);
            let cx = pf_x + col_w[..c].iter().sum::<i32>() + col_w[c] / 2;
            let cy = pf_y + row_h[..r].iter().sum::<i32>() + row_h[r] / 2;
            app_click(&mut app, cx, cy);
        }
        app_trigger_hint(&mut app);
        app_meta_mut(&mut app).set_status_message("Connected!", 1.5);

        for guide in [false, true] {
            for (mx, my) in [(160, 100), (24, 50)] {
                assert!(ref_renderer.render_if_needed(app.world_mut(), mx, my, false, guide));
                let composited = composite_gpu_layers_sw(&mut app, &mut layer_renderer, mx, my, guide);
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
            let pfx = app_meta(&app).playfield_x();
            let pfy = app_meta(&app).playfield_y();
            let p0 = app_piece_geom(&mut app, 0);
            let tx = pfx + p0.base_x + p0.base_w / 2;
            let ty = pfy + p0.base_y + p0.base_h / 2;
            assert!(app_right_click(&mut app, tx, ty));
        }
        for guide in [false, true] {
            for (mx, my) in [(150, 95), (38, 80)] {
                ref_renderer.render_if_needed(app.world_mut(), mx, my, false, guide);
                let composited = composite_gpu_layers_sw(&mut app, &mut layer_renderer, mx, my, guide);
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

    let mut app = build_game_app(test_game_dir(), false).unwrap();
    app_set_screen(&mut app, AppScreen::Browser);
    app.world_mut().resource_mut::<BrowserState>().selected_puzzle = 1;
    app.world_mut().resource_mut::<BrowserState>().selected_grid_dim = 2;
    app_start_selected_puzzle(&mut app);

    let tf = ViewportTransform::new(960.0, 720.0, 1.5, true);
    let mut router = InputRouter::new();
    let dos_to_logical = |dx: i32, dy: i32| -> (f32, f32) {
        (
            tf.viewport.x + (dx as f32 + 0.5) * tf.scale,
            tf.viewport.y + (dy as f32 + 0.5) * tf.scale,
        )
    };

    // 1. Touch drag on tray slot 0 picks up piece, drag to target places it
    let tray_pidx = app_tray_order(&mut app)[0];
    let (tx, ty) = dos_to_logical(23, 44);
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 1,
        x: tx,
        y: ty,
    });
    let f1 = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f1.app_input);
    assert_eq!(app_held_piece(&mut app), None);

    router.push_event(PlatformEvent::TouchMove {
        finger_id: 1,
        x: tx + 12.0,
        y: ty,
    });
    let f1b = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f1b.app_input);
    assert_eq!(app_held_piece(&mut app), Some(tray_pidx));

    let (dest_x, dest_y) = {
        let pfx = app_meta(&app).playfield_x();
        let pfy = app_meta(&app).playfield_y();
        let p = app_piece_geom(&mut app, tray_pidx);
        dos_to_logical(pfx + p.base_x + p.base_w / 2, pfy + p.base_y + p.base_h / 2)
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
    step_app(&mut app, &f2.app_input);
    assert_eq!(app_held_piece(&mut app), None);
    assert!(app_is_placed(&mut app, tray_pidx));
    assert!(app_meta(&app).toolbox_open, "after step 1");

    // 2. Two-finger vertical drag on empty board area scrolls tray
    let initial_scroll = app_meta(&app).tray_scroll;
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
    step_app(&mut app, &f_scroll.app_input);
    assert_eq!(app_meta(&app).tray_scroll, initial_scroll + 1);
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
    step_app(&mut app, &f_lp_down.app_input);
    assert_eq!(app_held_piece(&mut app), None);
    let f_lp = router.finish_frame(0.4, &tf, true, true);
    step_app(&mut app, &f_lp.app_input);
    assert_eq!(app_held_piece(&mut app), Some(tray_pidx));
    assert!(!app_is_placed(&mut app, tray_pidx));
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 4,
        x: dest_x,
        y: dest_y,
    });
    let f_lp_up = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_lp_up.app_input);
    // Piece remains held after long-press lift so user can tap destination or tray to return
    assert_eq!(app_held_piece(&mut app), Some(tray_pidx));

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
    step_app(&mut app, &f_ret.app_input);
    let f_ret_rel = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_ret_rel.app_input);
    assert_eq!(app_held_piece(&mut app), None);

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
    step_app(&mut app, &f_menu.app_input);
    let f_menu_rel = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_menu_rel.app_input);
    assert!(app.world().resource::<InGameMenuState>().open);

    // 6. Close modal menu, place a 2-piece edge-connected cluster, and verify both
    //    long-press and two-finger tap lift the entire 2-piece group (forwarding every frame).
    app_close_in_game_menu(&mut app, false);
    for pidx in [0usize, 1] {
        app_set_piece_held(&mut app, pidx);
        let pfx = app_meta(&app).playfield_x();
        let pfy = app_meta(&app).playfield_y();
        let p = app_piece_geom(&mut app, pidx);
        let px = pfx + p.base_x + p.base_w / 2;
        let py = pfy + p.base_y + p.base_h / 2;
        app_click(&mut app, px, py);
        assert!(app_held_piece(&mut app).is_none());
        assert!(app_is_placed(&mut app, pidx));
    }
    let (c_x, c_y) = {
        let pfx = app_meta(&app).playfield_x();
        let pfy = app_meta(&app).playfield_y();
        let p = app_piece_geom(&mut app, 0);
        dos_to_logical(pfx + p.base_x + p.base_w / 2, pfy + p.base_y + p.base_h / 2)
    };
    // 6a. Long-press lifts both pieces
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 7,
        x: c_x,
        y: c_y,
    });
    let f_c_down = router.finish_frame(0.1, &tf, true, true);
    step_app(&mut app, &f_c_down.app_input);
    assert_eq!(app_held_piece(&mut app), None);
    let f_c_lp = router.finish_frame(0.4, &tf, true, true);
    step_app(&mut app, &f_c_lp.app_input);
    assert_eq!(app_held_piece(&mut app), Some(0));
    assert_eq!(app_held_group_pieces(&mut app).len(), 2);
    assert!(!app_is_placed(&mut app, 0) && !app_is_placed(&mut app, 1));
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 7,
        x: c_x,
        y: c_y,
    });
    let f_c_up = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_c_up.app_input);

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
    step_app(&mut app, &f_place.app_input);
    let f_place_rel = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_place_rel.app_input);
    assert!(app_is_placed(&mut app, 0));
    assert!(app_is_placed(&mut app, 1));

    // 6b. Two-finger tap also lifts the entire 2-piece connected cluster
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 9,
        x: c_x,
        y: c_y,
    });
    let f_tf_down1 = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_tf_down1.app_input);
    assert_eq!(app_held_piece(&mut app), None);
    router.push_event(PlatformEvent::TouchDown {
        finger_id: 10,
        x: c_x + 30.0,
        y: c_y,
    });
    let f_tf_down2 = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_tf_down2.app_input);
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 10,
        x: c_x + 30.0,
        y: c_y,
    });
    let f_tf_up2 = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_tf_up2.app_input);
    assert_eq!(app_held_piece(&mut app), Some(0));
    assert_eq!(app_held_group_pieces(&mut app).len(), 2);
    assert!(!app_is_placed(&mut app, 0) && !app_is_placed(&mut app, 1));
    router.push_event(PlatformEvent::TouchUp {
        finger_id: 9,
        x: c_x,
        y: c_y,
    });
    let f_tf_up1 = router.finish_frame(0.016, &tf, true, true);
    step_app(&mut app, &f_tf_up1.app_input);
    assert_eq!(app_held_group_pieces(&mut app).len(), 2);
}

#[test]
fn bevy_state_time_and_ecs_invariants_without_assets() {
    use bevy_time::{Time, Virtual};

    let mut app = synthetic_app();
    let preview = app.world().resource::<BrowserState>().preview_asset.clone();
    app_spawn_puzzle(&mut app, 1, preview, 3, true, false, true);
    app_set_screen(&mut app, AppScreen::Playing);

    // 1. Time<Virtual> advances in Playing, pauses in InGameMenu and Setup.
    step_app(&mut app, &AppInput { dt: 0.5, ..Default::default() });
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    assert!((app_meta(&app).elapsed_secs - 0.5).abs() < 1e-6);

    app_open_in_game_menu(&mut app);
    step_app(&mut app, &AppInput { dt: 0.5, ..Default::default() });
    assert!(app.world().resource::<Time<Virtual>>().is_paused());
    assert!((app_meta(&app).elapsed_secs - 0.5).abs() < 1e-6);
    app_close_in_game_menu(&mut app, false);

    step_app(&mut app, &AppInput { f4: true, dt: 0.5, ..Default::default() });
    assert!(matches!(app_screen(&app), AppScreen::Setup { return_to_playing: true }));
    assert!(app.world().resource::<Time<Virtual>>().is_paused());
    assert!((app_meta(&app).elapsed_secs - 0.5).abs() < 1e-6);

    step_app(&mut app, &AppInput { escape: true, ..Default::default() });
    assert_eq!(app_screen(&app), AppScreen::Playing);
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());

    // 2. RenderVersions stays stable on idle frames in Playing.
    let v0 = *app.world().resource::<RenderVersions>();
    step_app(&mut app, &AppInput::default());
    let v1 = *app.world().resource::<RenderVersions>();
    assert_eq!(v0.app_rev, v1.app_rev);
    assert_eq!(v0.board_rev, v1.board_rev);
    assert_eq!(v0.tray_rev, v1.tray_rev);

    // 3. Single-source-of-truth PieceLocation partition invariant: tray + board + held == 9.
    app_click(&mut app, 20, 40);
    let (mut tray_n, mut board_n, mut held_n, mut anchor_n) = (0, 0, 0, 0);
    let w = app.world_mut();
    for loc in w.query::<&PieceLocation>().iter(w) {
        match *loc {
            PieceLocation::Tray { .. } => tray_n += 1,
            PieceLocation::Board { .. } => board_n += 1,
            PieceLocation::Held { is_anchor, .. } => {
                held_n += 1;
                if is_anchor {
                    anchor_n += 1;
                }
            }
        }
    }
    assert_eq!(tray_n + board_n + held_n, 9);
    assert_eq!(held_n, 1);
    assert_eq!(anchor_n, 1);
}
