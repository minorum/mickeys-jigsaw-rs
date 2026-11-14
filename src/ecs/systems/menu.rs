//! In-game overlay menu and exit/quit confirmation dialog system.

use bevy_ecs::prelude::*;
use bevy_state::prelude::{NextState, State};

use crate::app::{AppScreen, InGameMenuCommand, in_game_menu_command_at, in_game_menu_contains};
use crate::audio::CommonSound;
use crate::ecs::components::{AnimationState, PieceIdentity, PieceLocation};
use crate::ecs::events::AudioCommand;
use crate::ecs::puzzle_ops::{build_animation_state, default_playback, flush_audio_commands};
use crate::ecs::resources::{
    ActivePuzzle, AudioStatus, GameConfig, InGameMenuState, InputSnapshot, ScreenState, SetupState,
};
use crate::ecs::systems::browser::request_quit;
use crate::ecs::systems::screen::{open_setup_screen, set_screen};

pub fn is_menu_command_enabled(cmd: InGameMenuCommand, solved: bool) -> bool {
    match cmd {
        InGameMenuCommand::PieceTray => !solved,
        InGameMenuCommand::Replay => solved,
        _ => true,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn in_game_menu_system(
    mut commands: Commands,
    state: Res<State<AppScreen>>,
    mut next_state: ResMut<NextState<AppScreen>>,
    mut input_snap: ResMut<InputSnapshot>,
    mut screen_state: ResMut<ScreenState>,
    mut menu: ResMut<InGameMenuState>,
    mut active_puzzle: ResMut<ActivePuzzle>,
    mut setup: ResMut<SetupState>,
    config: Res<GameConfig>,
    audio_status: Res<AudioStatus>,
    mut piece_q: Query<(&PieceIdentity, &mut PieceLocation)>,
    anim_q: Query<Entity, With<AnimationState>>,
    mut audio_out: MessageWriter<AudioCommand>,
) {
    let cur_screen = *state.get();
    if input_snap.consumed_global || cur_screen != AppScreen::Playing {
        return;
    }
    let input = input_snap.input;
    let dt = input_snap.dt;

    // 1. Confirmation dialogs (Exit to Browser / Quit Game)
    if menu.confirm_exit_puzzle || menu.confirm_quit_puzzle {
        if input.enter || input.left_pressed {
            if menu.confirm_quit_puzzle {
                menu.confirm_quit_puzzle = false;
                if let Some(meta) = &mut active_puzzle.0 {
                    meta.status_message = None;
                }
                request_quit(&mut screen_state, &mut audio_out);
            } else {
                menu.confirm_exit_puzzle = false;
                do_return_to_browser(
                    cur_screen,
                    &mut next_state,
                    &mut menu,
                    &mut active_puzzle,
                    &mut audio_out,
                );
            }
        } else if input.escape {
            menu.confirm_exit_puzzle = false;
            menu.confirm_quit_puzzle = false;
            audio_out.write(AudioCommand::PlayCommon(CommonSound::Cancel));
            if let Some(meta) = &mut active_puzzle.0 {
                meta.status_message = None;
            }
        }
        input_snap.consumed_global = true;
        return;
    }

    if !menu.open {
        return;
    }

    // 2. Open In-Game Menu navigation & clicks
    if let Some(meta) = &mut active_puzzle.0 {
        if input.pointer_moved && input.move_x == 0 && input.move_y == 0 && !input.enter {
            meta.keyboard_pointer = None;
        }
        if !input.left_pressed && !input.right_pressed && (input.move_x != 0 || input.move_y != 0)
        {
            move_keyboard_pointer_meta(meta, input.move_x as f32, input.move_y as f32, dt);
        }
    }

    let (px, py) = active_puzzle
        .0
        .as_ref()
        .map_or((input.pointer_x, input.pointer_y), |m| {
            m.active_pointer(input.pointer_x, input.pointer_y)
        });
    let solved = active_puzzle.0.as_ref().is_some_and(|m| m.solved);

    if (input.pointer_moved || input.move_x != 0 || input.move_y != 0)
        && let Some(cmd) = in_game_menu_command_at(px, py)
        && is_menu_command_enabled(cmd, solved)
        && let Some(idx) = InGameMenuCommand::ALL.iter().position(|&c| c == cmd)
        && menu.selected != idx
    {
        menu.selected = idx;
    }

    if input.escape {
        menu.open = false;
        menu.pressed = None;
        audio_out.write(AudioCommand::PlayCommon(CommonSound::Cancel));
        input_snap.consumed_global = true;
        return;
    }

    if input.up {
        step_menu_selection(-1, &mut menu, solved, &mut audio_out);
        input_snap.consumed_global = true;
        return;
    }
    if input.down {
        step_menu_selection(1, &mut menu, solved, &mut audio_out);
        input_snap.consumed_global = true;
        return;
    }
    if input.key_r && is_menu_command_enabled(InGameMenuCommand::Replay, solved) {
        execute_menu_command(
            InGameMenuCommand::Replay,
            &mut commands,
            cur_screen,
            &mut next_state,
            &mut screen_state,
            &mut menu,
            &mut active_puzzle,
            &mut setup,
            &config,
            &audio_status,
            &mut piece_q,
            &anim_q,
            &mut audio_out,
        );
        input_snap.consumed_global = true;
        return;
    }
    if input.key_q {
        execute_menu_command(
            InGameMenuCommand::Quit,
            &mut commands,
            cur_screen,
            &mut next_state,
            &mut screen_state,
            &mut menu,
            &mut active_puzzle,
            &mut setup,
            &config,
            &audio_status,
            &mut piece_q,
            &anim_q,
            &mut audio_out,
        );
        input_snap.consumed_global = true;
        return;
    }
    if input.enter || input.space {
        let cmd = InGameMenuCommand::ALL[menu.selected];
        execute_menu_command(
            cmd,
            &mut commands,
            cur_screen,
            &mut next_state,
            &mut screen_state,
            &mut menu,
            &mut active_puzzle,
            &mut setup,
            &config,
            &audio_status,
            &mut piece_q,
            &anim_q,
            &mut audio_out,
        );
        input_snap.consumed_global = true;
        return;
    }
    if input.left_pressed {
        if let Some(meta) = &mut active_puzzle.0 {
            meta.keyboard_pointer = None;
        }
        if let Some(cmd) = in_game_menu_command_at(input.pointer_x, input.pointer_y) {
            if is_menu_command_enabled(cmd, solved) {
                menu.pressed = Some(cmd);
                if let Some(idx) = InGameMenuCommand::ALL.iter().position(|&c| c == cmd) {
                    menu.selected = idx;
                }
            } else {
                audio_out.write(AudioCommand::PlayError);
            }
        } else if !in_game_menu_contains(input.pointer_x, input.pointer_y) {
            menu.open = false;
            menu.pressed = None;
            audio_out.write(AudioCommand::PlayCommon(CommonSound::Cancel));
        }
        input_snap.consumed_global = true;
        return;
    }
    if input.left_released {
        if let Some(meta) = &mut active_puzzle.0 {
            meta.keyboard_pointer = None;
        }
        if let Some(pressed_cmd) = menu.pressed.take()
            && in_game_menu_command_at(input.pointer_x, input.pointer_y) == Some(pressed_cmd)
        {
            execute_menu_command(
                pressed_cmd,
                &mut commands,
                cur_screen,
                &mut next_state,
                &mut screen_state,
                &mut menu,
                &mut active_puzzle,
                &mut setup,
                &config,
                &audio_status,
                &mut piece_q,
                &anim_q,
                &mut audio_out,
            );
        }
        input_snap.consumed_global = true;
        return;
    }

    input_snap.consumed_global = true;
}

pub fn move_keyboard_pointer_meta(
    meta: &mut crate::ecs::resources::PuzzleMeta,
    dx: f32,
    dy: f32,
    dt: f32,
) {
    meta.drag_origin = None;
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let (x, y) = meta.keyboard_pointer.unwrap_or((20.0, 40.0));
    let step = 120.0 * dt / dx.hypot(dy).max(1.0);
    let next = (
        (x + dx * step).clamp(0.0, 319.0),
        (y + dy * step).clamp(0.0, 199.0),
    );
    meta.keyboard_pointer = Some(next);
}

pub fn do_return_to_browser(
    cur_screen: AppScreen,
    next_state: &mut NextState<AppScreen>,
    menu: &mut InGameMenuState,
    active_puzzle: &mut ActivePuzzle,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    if let Some(meta) = &mut active_puzzle.0 {
        meta.drag_origin = None;
    }
    menu.confirm_exit_puzzle = false;
    menu.confirm_quit_puzzle = false;
    menu.open = false;
    menu.pressed = None;
    audio_out.write(AudioCommand::StopAll);
    audio_out.write(AudioCommand::PlayCommon(CommonSound::OtherPicture));
    set_screen(cur_screen, next_state, AppScreen::Browser, audio_out);
}

pub fn return_to_browser(
    cur_screen: AppScreen,
    next_state: &mut NextState<AppScreen>,
    menu: &mut InGameMenuState,
    active_puzzle: &mut ActivePuzzle,
    piece_q: &Query<(&PieceIdentity, &mut PieceLocation)>,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    menu.open = false;
    menu.pressed = None;
    menu.confirm_quit_puzzle = false;
    let has_progress = piece_q
        .iter()
        .any(|(_, loc)| loc.is_placed() || matches!(*loc, PieceLocation::Held { group_order, .. } if group_order > 0));
    let held_count = piece_q.iter().filter(|(_, loc)| loc.is_held()).count();
    let unfinished = active_puzzle
        .0
        .as_ref()
        .is_some_and(|m| !m.solved && (has_progress || held_count > 1));

    if unfinished && !menu.confirm_exit_puzzle {
        menu.confirm_exit_puzzle = true;
        audio_out.write(AudioCommand::PlayCommon(CommonSound::NotDoneYet));
        if let Some(meta) = &mut active_puzzle.0 {
            meta.drag_origin = None;
            meta.set_status_message(
                "We're not done yet. Exit? (Enter/Click=Yes, Esc=No)",
                8.0,
            );
        }
        return;
    }
    do_return_to_browser(cur_screen, next_state, menu, active_puzzle, audio_out);
}

pub fn request_quit_from_puzzle(
    menu: &mut InGameMenuState,
    active_puzzle: &mut ActivePuzzle,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    menu.open = false;
    menu.pressed = None;
    menu.confirm_exit_puzzle = false;
    if menu.confirm_quit_puzzle {
        return;
    }
    menu.confirm_quit_puzzle = true;
    let solved = active_puzzle.0.as_ref().is_some_and(|m| m.solved);
    if !solved {
        audio_out.write(AudioCommand::PlayCommon(CommonSound::NotDoneYet));
    }
    if let Some(meta) = &mut active_puzzle.0 {
        meta.drag_origin = None;
        meta.set_status_message("Quit game? (Enter/Click=Yes, Esc=No)", 8.0);
    }
}

fn step_menu_selection(
    delta: isize,
    menu: &mut InGameMenuState,
    solved: bool,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    let len = InGameMenuCommand::ALL.len() as isize;
    let mut idx = menu.selected as isize;
    for _ in 0..len {
        idx = (idx + delta).rem_euclid(len);
        let cmd = InGameMenuCommand::ALL[idx as usize];
        if is_menu_command_enabled(cmd, solved) {
            menu.selected = idx as usize;
            audio_out.write(AudioCommand::PlayUiClick);
            return;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_menu_command(
    cmd: InGameMenuCommand,
    commands: &mut Commands,
    cur_screen: AppScreen,
    next_state: &mut NextState<AppScreen>,
    _screen_state: &mut ScreenState,
    menu: &mut InGameMenuState,
    active_puzzle: &mut ActivePuzzle,
    setup: &mut SetupState,
    config: &GameConfig,
    audio_status: &AudioStatus,
    piece_q: &mut Query<(&PieceIdentity, &mut PieceLocation)>,
    anim_q: &Query<Entity, With<AnimationState>>,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    let solved = active_puzzle.0.as_ref().is_some_and(|m| m.solved);
    if !is_menu_command_enabled(cmd, solved) {
        audio_out.write(AudioCommand::PlayError);
        return;
    }
    menu.open = false;
    menu.pressed = None;
    match cmd {
        InGameMenuCommand::OtherPicture => {
            return_to_browser(cur_screen, next_state, menu, active_puzzle, piece_q, audio_out);
        }
        InGameMenuCommand::Setup => {
            open_setup_screen(true, cur_screen, next_state, menu, setup, config, audio_out);
        }
        InGameMenuCommand::PieceTray => {
            if let Some(meta) = &mut active_puzzle.0 {
                let held_count = piece_q.iter().filter(|(_, loc)| loc.is_held()).count();
                if held_count > 0 {
                    if held_count == 1 {
                        return_held_to_tray_at(0, meta, piece_q, audio_out);
                        meta.tray_scroll = 0;
                        meta.toolbox_open = true;
                    } else {
                        audio_out.write(AudioCommand::PlayError);
                    }
                } else {
                    meta.drag_origin = None;
                    meta.pressed_toolbox_target = None;
                    meta.toolbox_open = !meta.toolbox_open;
                    audio_out.write(AudioCommand::PlayUiClick);
                }
            }
        }
        InGameMenuCommand::Replay => {
            if let Some(meta) = &mut active_puzzle.0
                && meta.solved
            {
                meta.pending_completion_feedback = false;
                audio_out.write(AudioCommand::StopAll);
                audio_out.write(AudioCommand::PlayCommon(CommonSound::SeeWhatHappens));
                for e in anim_q.iter() {
                    commands.entity(e).despawn();
                }
                let playback = default_playback(&meta.asset);
                let mut cmds = Vec::new();
                if let Some(anim) = build_animation_state(meta, playback, audio_status, &mut cmds) {
                    flush_audio_commands(cmds, audio_out);
                    commands.spawn(anim);
                }
            }
        }
        InGameMenuCommand::Quit => {
            request_quit_from_puzzle(menu, active_puzzle, audio_out);
        }
        InGameMenuCommand::Resume => {
            audio_out.write(AudioCommand::PlayCommon(CommonSound::Cancel));
        }
    }
}

pub fn return_held_to_tray_at(
    insert_at: usize,
    meta: &mut crate::ecs::resources::PuzzleMeta,
    piece_q: &mut Query<(&PieceIdentity, &mut PieceLocation)>,
    audio_out: &mut MessageWriter<AudioCommand>,
) -> bool {
    let tray_len = piece_q.iter().filter(|(_, loc)| loc.is_tray()).count();
    let mut held_count = 0usize;
    for (_, loc) in piece_q.iter() {
        if loc.is_held() {
            held_count += 1;
        }
    }
    if held_count == 0 {
        return false;
    }
    let insert_at = insert_at.min(tray_len);

    // Shift existing tray pieces at or after `insert_at`
    for (_, mut loc) in piece_q.iter_mut() {
        if let PieceLocation::Tray { order } = *loc
            && order >= insert_at
        {
            *loc = PieceLocation::Tray {
                order: order + held_count,
            };
        }
    }

    // Move held pieces into tray preserving `group_order`
    for (_, mut loc) in piece_q.iter_mut() {
        if let PieceLocation::Held { group_order, .. } = *loc {
            *loc = PieceLocation::Tray {
                order: insert_at + group_order,
            };
        }
    }

    meta.held_group_origin_cell = None;
    audio_out.write(AudioCommand::PlayCommon(CommonSound::ReturnPiece));
    true
}
