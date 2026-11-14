//! Global shortcuts, quit timer, pointer-cancel, and Title/Credits screen transitions.

use bevy_ecs::prelude::*;
use bevy_state::prelude::{NextState, State};
use bevy_time::{Real, Time, Virtual};

use crate::app::AppScreen;
use crate::ecs::events::AudioCommand;
use crate::ecs::resources::{
    ActivePuzzle, BrowserState, GameConfig, InGameMenuState, InputSnapshot, ScreenState, SetupState,
    StaticAssets,
};

pub fn in_setup(state: Res<State<AppScreen>>) -> bool {
    matches!(state.get(), AppScreen::Setup { .. })
}

pub fn in_credits(state: Res<State<AppScreen>>) -> bool {
    matches!(state.get(), AppScreen::Credits { .. })
}

pub fn reset_screen_timer_system(mut screen_state: ResMut<ScreenState>) {
    screen_state.screen_timer.reset();
}

pub fn sync_virtual_time_pause_system(
    state: Res<State<AppScreen>>,
    menu: Res<InGameMenuState>,
    mut vtime: ResMut<Time<Virtual>>,
) {
    let should_pause = *state.get() != AppScreen::Playing
        || menu.open
        || menu.confirm_exit_puzzle
        || menu.confirm_quit_puzzle;
    if should_pause {
        vtime.pause();
    } else {
        vtime.unpause();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn global_and_screen_system(
    real_time: Res<Time<Real>>,
    state: Res<State<AppScreen>>,
    mut next_state: ResMut<NextState<AppScreen>>,
    mut input_snap: ResMut<InputSnapshot>,
    mut screen_state: ResMut<ScreenState>,
    mut browser: ResMut<BrowserState>,
    mut menu: ResMut<InGameMenuState>,
    mut active_puzzle: ResMut<ActivePuzzle>,
    mut setup: ResMut<SetupState>,
    config: Res<GameConfig>,
    assets: Res<StaticAssets>,
    mut audio_out: MessageWriter<AudioCommand>,
) {
    let dt = real_time.delta_secs();
    input_snap.dt = dt;
    input_snap.consumed_global = false;

    if screen_state.quit_requested {
        screen_state.quit_timer.tick(real_time.delta());
        input_snap.consumed_global = true;
        return;
    }

    let input = input_snap.input;
    if input.pointer_canceled {
        browser.pressed = None;
        menu.pressed = None;
        if let Some(meta) = &mut active_puzzle.0 {
            meta.drag_origin = None;
        }
    }

    let cur_screen = *state.get();

    // Global shortcut F4 opens Setup from anywhere
    if input.f4 && !matches!(cur_screen, AppScreen::Setup { .. }) {
        if let Some(meta) = &mut active_puzzle.0 {
            meta.drag_origin = None;
        }
        let in_play = cur_screen == AppScreen::Playing;
        open_setup_screen(
            in_play,
            cur_screen,
            &mut next_state,
            &mut menu,
            &mut setup,
            &config,
            &mut audio_out,
        );
        input_snap.consumed_global = true;
        return;
    }

    match cur_screen {
        AppScreen::Title => {
            screen_state.screen_timer.tick(real_time.delta());
            if input.escape {
                set_screen(cur_screen, &mut next_state, AppScreen::Browser, &mut audio_out);
            } else if input.left_pressed
                || input.space
                || input.enter
                || screen_state.screen_timer.elapsed_secs() > 8.0
            {
                let next = if assets.credit_images.is_empty() {
                    AppScreen::Browser
                } else {
                    AppScreen::Credits { page: 0 }
                };
                set_screen(cur_screen, &mut next_state, next, &mut audio_out);
            }
            input_snap.consumed_global = true;
        }
        AppScreen::Credits { page } => {
            screen_state.screen_timer.tick(real_time.delta());
            if input.escape {
                set_screen(cur_screen, &mut next_state, AppScreen::Browser, &mut audio_out);
            } else if input.left_pressed
                || input.space
                || input.enter
                || screen_state.screen_timer.elapsed_secs() > 5.0
            {
                let next = if page + 1 < assets.credit_images.len() {
                    AppScreen::Credits { page: page + 1 }
                } else {
                    AppScreen::Browser
                };
                set_screen(cur_screen, &mut next_state, next, &mut audio_out);
            }
            input_snap.consumed_global = true;
        }
        _ => {}
    }
}

pub fn set_screen(
    current: AppScreen,
    next_state: &mut NextState<AppScreen>,
    next: AppScreen,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    if current != next {
        next_state.set(next);
        audio_out.write(AudioCommand::SetPaused(matches!(
            next,
            AppScreen::Setup { .. }
        )));
    }
}

pub fn open_setup_screen(
    return_to_playing: bool,
    current: AppScreen,
    next_state: &mut NextState<AppScreen>,
    menu: &mut InGameMenuState,
    setup: &mut SetupState,
    config: &GameConfig,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    menu.open = false;
    menu.pressed = None;
    menu.confirm_exit_puzzle = false;
    menu.confirm_quit_puzzle = false;
    setup.menu.set_prefs(config.prefs);
    setup.menu.clear_error();
    set_screen(
        current,
        next_state,
        AppScreen::Setup { return_to_playing },
        audio_out,
    );
}
