//! Puzzle Browser screen input system and puzzle spawning.

use std::sync::atomic::{AtomicU64, Ordering};

use bevy_ecs::prelude::*;
use bevy_state::prelude::{NextState, State};
use bevy_time::{Real, Time};

use crate::app::{AppScreen, BrowserAction, BrowserTarget};
use crate::assets::PuzzleAsset;
use crate::audio::CommonSound;
use crate::ecs::components::{AnimationState, PieceIdentity, PieceLocation, SceneHotspot};
use crate::ecs::events::AudioCommand;
use crate::ecs::puzzle_ops::{cut_pieces_data, extract_hotspots, render_full_picture};
use crate::ecs::resources::{
    ActivePuzzle, BrowserState, GameConfig, InGameMenuState, InputSnapshot, PuzzleMeta,
    ScreenState, SetupState,
};
use crate::ecs::systems::screen::{open_setup_screen, set_screen};
use crate::puzzle::{LcgRng, export_print_pbm};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

pub fn can_resume_selected(
    active_puzzle: &ActivePuzzle,
    browser: &BrowserState,
    config: &GameConfig,
) -> bool {
    active_puzzle.0.as_ref().is_some_and(|meta| {
        !meta.solved
            && meta.puzzle_num == browser.selected_puzzle
            && meta.grid_dim == browser.selected_grid_dim
            && (meta.grid_dim == 1 || meta.curved_edges == browser.curved_edges)
            && config.session_use_ega == (config.prefs.video_mode >= 2)
    })
}

#[allow(clippy::too_many_arguments)]
pub fn browser_input_system(
    mut commands: Commands,
    real_time: Res<Time<Real>>,
    state: Res<State<AppScreen>>,
    mut next_state: ResMut<NextState<AppScreen>>,
    input_snap: Res<InputSnapshot>,
    mut screen_state: ResMut<ScreenState>,
    mut browser: ResMut<BrowserState>,
    mut config: ResMut<GameConfig>,
    mut setup: ResMut<SetupState>,
    mut menu: ResMut<InGameMenuState>,
    mut active_puzzle: ResMut<ActivePuzzle>,
    piece_q: Query<Entity, With<PieceIdentity>>,
    hotspot_q: Query<Entity, With<SceneHotspot>>,
    anim_q: Query<Entity, With<AnimationState>>,
    mut audio_out: MessageWriter<AudioCommand>,
) {
    let cur_screen = *state.get();
    if input_snap.consumed_global || cur_screen != AppScreen::Browser {
        return;
    }
    let input = input_snap.input;

    if browser.status.is_some()
        && let Some((_, ref mut timer)) = browser.status
    {
        timer.tick(real_time.delta());
        if timer.is_finished() {
            browser.status = None;
        }
    }
    if input.pointer_moved {
        browser.hover = BrowserTarget::hit_test(input.pointer_x, input.pointer_y);
    }
    if input.left_released {
        browser.pressed = None;
    }

    if input.left {
        step_puzzle(-1, &mut browser, &config, &mut screen_state, &mut audio_out);
    } else if input.right {
        step_puzzle(1, &mut browser, &config, &mut screen_state, &mut audio_out);
    } else if input.up {
        step_grid_dim(-1, &mut browser, &active_puzzle, &config, &mut audio_out);
    } else if input.down {
        step_grid_dim(1, &mut browser, &active_puzzle, &config, &mut audio_out);
    } else if input.tab || input.key_e {
        if browser.selected_grid_dim != 1 {
            browser.curved_edges = !browser.curved_edges;
        }
    } else if input.enter || input.space {
        start_selected_puzzle(
            &mut commands,
            cur_screen,
            &mut next_state,
            &mut screen_state,
            &browser,
            &mut config,
            &mut active_puzzle,
            &piece_q,
            &hotspot_q,
            &anim_q,
            &mut audio_out,
        );
    } else if input.key_r {
        restart_selected_puzzle(
            &mut commands,
            cur_screen,
            &mut next_state,
            &mut screen_state,
            &browser,
            &mut config,
            &mut active_puzzle,
            &piece_q,
            &hotspot_q,
            &anim_q,
            &mut audio_out,
        );
    } else if input.key_s {
        open_setup_screen(
            false,
            cur_screen,
            &mut next_state,
            &mut menu,
            &mut setup,
            &config,
            &mut audio_out,
        );
    } else if input.key_p {
        print_selected_puzzle(&mut browser, &config, &mut audio_out);
    } else if input.escape || input.key_q {
        request_quit(&mut screen_state, &mut audio_out);
    } else if input.left_pressed {
        let target = BrowserTarget::hit_test(input.pointer_x, input.pointer_y);
        browser.hover = target;
        browser.pressed = target;
        match target {
            Some(BrowserTarget::Button(action)) => match action {
                BrowserAction::PrevPuzzle => {
                    step_puzzle(-1, &mut browser, &config, &mut screen_state, &mut audio_out)
                }
                BrowserAction::NextPuzzle => {
                    step_puzzle(1, &mut browser, &config, &mut screen_state, &mut audio_out)
                }
                BrowserAction::CyclePieces => {
                    step_grid_dim(1, &mut browser, &active_puzzle, &config, &mut audio_out)
                }
                BrowserAction::ToggleEdges => {
                    if browser.selected_grid_dim != 1 {
                        browser.curved_edges = !browser.curved_edges;
                    }
                }
                BrowserAction::OpenSetup => {
                    audio_out.write(AudioCommand::PlayCommon(CommonSound::SetupStart));
                    open_setup_screen(
                        false,
                        cur_screen,
                        &mut next_state,
                        &mut menu,
                        &mut setup,
                        &config,
                        &mut audio_out,
                    );
                }
                BrowserAction::StartOrResume => start_selected_puzzle(
                    &mut commands,
                    cur_screen,
                    &mut next_state,
                    &mut screen_state,
                    &browser,
                    &mut config,
                    &mut active_puzzle,
                    &piece_q,
                    &hotspot_q,
                    &anim_q,
                    &mut audio_out,
                ),
                BrowserAction::PrintPuzzle => {
                    print_selected_puzzle(&mut browser, &config, &mut audio_out)
                }
                BrowserAction::QuitGame => request_quit(&mut screen_state, &mut audio_out),
            },
            Some(BrowserTarget::PuzzleSlot(puzzle_id)) => {
                let clamped = puzzle_id.clamp(1, 15);
                if browser.selected_puzzle != clamped {
                    browser.selected_puzzle = clamped;
                    reload_preview(&mut browser, &config, &mut screen_state);
                }
                audio_out.write(AudioCommand::PlayCommon(CommonSound::UiClick));
            }
            None => {
                if (34..302).contains(&input.pointer_x) && (22..158).contains(&input.pointer_y) {
                    start_selected_puzzle(
                        &mut commands,
                        cur_screen,
                        &mut next_state,
                        &mut screen_state,
                        &browser,
                        &mut config,
                        &mut active_puzzle,
                        &piece_q,
                        &hotspot_q,
                        &anim_q,
                        &mut audio_out,
                    );
                }
            }
        }
    }
}

pub fn clear_browser_pointer_on_exit(mut browser: ResMut<BrowserState>) {
    browser.hover = None;
    browser.pressed = None;
}

pub fn request_quit(screen_state: &mut ScreenState, audio_out: &mut MessageWriter<AudioCommand>) {
    if screen_state.quit_requested {
        return;
    }
    audio_out.write(AudioCommand::PlayCommon(CommonSound::Quit));
    screen_state.quit_requested = true;
    screen_state.quit_timer.reset();
}

fn reload_preview(browser: &mut BrowserState, config: &GameConfig, screen_state: &mut ScreenState) {
    let use_ega = config.prefs.video_mode >= 2;
    match PuzzleAsset::load(&config.game_dir, browser.selected_puzzle, use_ega) {
        Ok(asset) => {
            browser.preview_asset = asset;
            screen_state.error = None;
        }
        Err(err) => {
            eprintln!("{err}");
            browser.selected_puzzle = browser.preview_asset.id;
            screen_state.error = Some(err);
        }
    }
}

fn step_puzzle(
    delta: i32,
    browser: &mut BrowserState,
    config: &GameConfig,
    screen_state: &mut ScreenState,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    browser.selected_puzzle = if delta < 0 {
        if browser.selected_puzzle > 1 {
            browser.selected_puzzle - 1
        } else {
            15
        }
    } else {
        (browser.selected_puzzle % 15) + 1
    };
    reload_preview(browser, config, screen_state);
    audio_out.write(AudioCommand::PlayCommon(if delta < 0 {
        CommonSound::PreviousPicture
    } else {
        CommonSound::NextPicture
    }));
}

fn step_grid_dim(
    delta: i32,
    browser: &mut BrowserState,
    active_puzzle: &ActivePuzzle,
    config: &GameConfig,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    browser.selected_grid_dim = if delta > 0 {
        (browser.selected_grid_dim % 8) + 1
    } else if browser.selected_grid_dim > 1 {
        browser.selected_grid_dim - 1
    } else {
        8
    };
    audio_out.write(AudioCommand::PlayCommon(
        if can_resume_selected(active_puzzle, browser, config) {
            CommonSound::ChangePieceSize
        } else {
            CommonSound::ChooseSize
        },
    ));
}

fn print_selected_puzzle(
    browser: &mut BrowserState,
    config: &GameConfig,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    let msg = match export_print_pbm(&config.game_dir, browser.selected_puzzle) {
        Ok(ok_msg) => {
            audio_out.write(AudioCommand::PlayCommon(CommonSound::UiClick));
            ok_msg
        }
        Err(err_msg) => {
            audio_out.write(AudioCommand::PlayError);
            err_msg
        }
    };
    browser.set_status(msg, 3.0);
}

#[allow(clippy::too_many_arguments)]
fn start_selected_puzzle(
    commands: &mut Commands,
    cur_screen: AppScreen,
    next_state: &mut NextState<AppScreen>,
    screen_state: &mut ScreenState,
    browser: &BrowserState,
    config: &mut GameConfig,
    active_puzzle: &mut ActivePuzzle,
    piece_q: &Query<Entity, With<PieceIdentity>>,
    hotspot_q: &Query<Entity, With<SceneHotspot>>,
    anim_q: &Query<Entity, With<AnimationState>>,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    if can_resume_selected(active_puzzle, browser, config) {
        audio_out.write(AudioCommand::StopAll);
        screen_state.error = None;
        set_screen(cur_screen, next_state, AppScreen::Playing, audio_out);
    } else {
        restart_selected_puzzle(
            commands,
            cur_screen,
            next_state,
            screen_state,
            browser,
            config,
            active_puzzle,
            piece_q,
            hotspot_q,
            anim_q,
            audio_out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn restart_selected_puzzle(
    commands: &mut Commands,
    cur_screen: AppScreen,
    next_state: &mut NextState<AppScreen>,
    screen_state: &mut ScreenState,
    browser: &BrowserState,
    config: &mut GameConfig,
    active_puzzle: &mut ActivePuzzle,
    piece_q: &Query<Entity, With<PieceIdentity>>,
    hotspot_q: &Query<Entity, With<SceneHotspot>>,
    anim_q: &Query<Entity, With<AnimationState>>,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    let use_ega = config.prefs.video_mode >= 2;
    match PuzzleAsset::load(&config.game_dir, browser.selected_puzzle, use_ega) {
        Ok(asset) => {
            spawn_puzzle_session(
                commands,
                active_puzzle,
                piece_q,
                hotspot_q,
                anim_q,
                browser.selected_puzzle,
                asset,
                browser.selected_grid_dim,
                browser.curved_edges,
                config.prefs.grid_lines_on != 0,
                config.prefs.timer_on != 0,
                config.prefs.text_msg_on != 0,
            );
            config.session_use_ega = use_ega;
            audio_out.write(AudioCommand::PlayCommon(CommonSound::StartPuzzle));
            screen_state.error = None;
            set_screen(cur_screen, next_state, AppScreen::Playing, audio_out);
        }
        Err(err) => {
            eprintln!("{err}");
            screen_state.error = Some(err);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_puzzle_session(
    commands: &mut Commands,
    active_puzzle: &mut ActivePuzzle,
    piece_q: &Query<Entity, With<PieceIdentity>>,
    hotspot_q: &Query<Entity, With<SceneHotspot>>,
    anim_q: &Query<Entity, With<AnimationState>>,
    puzzle_num: usize,
    asset: PuzzleAsset,
    grid_dim: usize,
    curved_edges: bool,
    grid_lines: bool,
    timer_enabled: bool,
    text_enabled: bool,
) {
    for e in piece_q.iter() {
        commands.entity(e).despawn();
    }
    for e in hotspot_q.iter() {
        commands.entity(e).despawn();
    }
    for e in anim_q.iter() {
        commands.entity(e).despawn();
    }

    let dim = grid_dim.clamp(1, 8);
    let full_picture_fb = render_full_picture(&asset);
    let (cut_data, grid_polylines_v, grid_polylines_h) =
        cut_pieces_data(puzzle_num, &asset, &full_picture_fb, dim, curved_edges);

    let mut shuffle_rng = if dim == 1 {
        LcgRng::new(
            (puzzle_num as u32)
                .wrapping_mul(1103515245)
                .wrapping_add(12345),
        )
    } else {
        LcgRng::new(
            (puzzle_num as u32)
                .wrapping_mul(1103515245)
                .wrapping_add((dim as u32) * 12345),
        )
    };
    let tray_order = shuffle_rng.shuffled_indices(cut_data.len());
    let mut rank_by_idx = vec![0usize; cut_data.len()];
    for (rank, &piece_idx) in tray_order.iter().enumerate() {
        rank_by_idx[piece_idx] = rank;
    }

    for piece in cut_data {
        let rank = rank_by_idx[piece.identity.index];
        commands.spawn((
            piece.identity,
            piece.geometry,
            piece.edges,
            crate::ecs::components::PieceSprite {
                sprite: piece.sprite,
            },
            PieceLocation::Tray { order: rank },
        ));
    }

    for hotspot in extract_hotspots(&asset) {
        commands.spawn(hotspot);
    }

    active_puzzle.0 = Some(PuzzleMeta {
        session_id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
        puzzle_num,
        asset,
        asset_rev: 1,
        grid_dim: dim,
        curved_edges,
        toolbox_open: true,
        grid_lines,
        timer_enabled,
        text_enabled,
        elapsed_secs: 0.0,
        tray_scroll: 0,
        keyboard_pointer: None,
        drag_origin: None,
        held_group_origin_cell: None,
        pressed_toolbox_target: None,
        solved: false,
        pending_completion_feedback: false,
        voice_rng: LcgRng::new((puzzle_num as u32).wrapping_mul(0x9e37_79b9) | 1),
        status_message: None,
        full_picture_fb,
        grid_polylines_v,
        grid_polylines_h,
    });
}
