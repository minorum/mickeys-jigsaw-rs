//! Modular Bevy plugins organizing gameplay, state transitions, timing, audio, and render tracking.

use std::time::Duration;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_state::app::StatesPlugin;
use bevy_state::prelude::*;
use bevy_time::{Real, Time, TimePlugin, TimeUpdateStrategy};

use crate::app::AppScreen;
use crate::audio::AudioEngine;
use crate::ecs::events::AudioCommand;
use crate::ecs::resources::{
    ActivePuzzle, AudioStatus, InGameMenuState, InputSnapshot, RenderVersions, ScreenState,
};
use crate::ecs::systems::{
    GameSet, browser, change_tracking, menu, puzzle_completion, puzzle_input, puzzle_time, screen,
    setup,
};

/// Exclusive system in `PostUpdate` that flushes any `NextState<AppScreen>` queued during `Update`
/// so `State<AppScreen>`, `OnEnter`/`OnExit`, audio pause, and `RenderVersions` are up to date
/// by the end of the same frame.
fn flush_state_transitions_system(world: &mut World) {
    if world
        .get_resource::<NextState<AppScreen>>()
        .is_some_and(|n| !matches!(n, NextState::Unchanged))
    {
        let _ = world.try_run_schedule(StateTransition);
    }
}

/// Resets `screen_timer` whenever `State<AppScreen>` transitions.
fn reset_screen_timer_on_transition(
    state: Res<State<AppScreen>>,
    mut screen_state: ResMut<ScreenState>,
) {
    if state.is_changed() {
        screen_state.screen_timer.reset();
    }
}

/// Allows `Time<Virtual>` to advance by arbitrary deterministic `dt` values in tests
/// without clamping to the default 250ms `max_delta`.
fn init_virtual_time_system(mut vtime: ResMut<bevy_time::Time<bevy_time::Virtual>>) {
    vtime.set_max_delta(Duration::MAX);
}

/// Core application plugin: `StatesPlugin`, `TimePlugin`, `AudioCommand` messages,
/// global shortcuts (`F4`, quit timer), and `Title` / `Credits` screen progression.
pub struct CoreGamePlugin {
    pub initial_screen: AppScreen,
}

impl Plugin for CoreGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((StatesPlugin, TimePlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .insert_state(self.initial_screen)
            .add_message::<AudioCommand>()
            .init_resource::<ScreenState>()
            .init_resource::<InputSnapshot>()
            .init_resource::<ActivePuzzle>()
            .configure_sets(
                Update,
                (
                    GameSet::GlobalAndScreenTransitions,
                    GameSet::MenuAndBrowserInput,
                    GameSet::PuzzleTimersAndAnimation,
                    GameSet::PuzzleInputAndActions,
                    GameSet::PuzzleCompletion,
                    GameSet::SyncAndChangeTracking,
                )
                    .chain(),
            )
            .add_systems(
                Startup,
                (
                    init_virtual_time_system,
                    screen::sync_virtual_time_pause_system,
                )
                    .chain(),
            )
            .add_systems(
                First,
                screen::sync_virtual_time_pause_system.before(bevy_time::TimeSystems),
            )
            .add_systems(
                Update,
                (
                    screen::sync_virtual_time_pause_system,
                    screen::global_and_screen_system,
                )
                    .chain()
                    .in_set(GameSet::GlobalAndScreenTransitions),
            )
            .add_systems(
                PostUpdate,
                (
                    flush_state_transitions_system,
                    reset_screen_timer_on_transition,
                    screen::sync_virtual_time_pause_system,
                )
                    .chain(),
            );
    }
}

/// Synchronizes `AudioEngine` (stored as `NonSend`) with `AudioStatus` in `PreUpdate`
/// and drains `AudioCommand` messages in `PostUpdate`.
pub struct AudioSyncPlugin;

fn pre_update_audio_system(
    real_time: Res<Time<Real>>,
    state: Res<State<AppScreen>>,
    mut audio: NonSendMut<AudioEngine>,
    mut status: ResMut<AudioStatus>,
) {
    if !matches!(state.get(), AppScreen::Setup { .. }) {
        audio.update(real_time.delta_secs());
    }
    sync_audio_status(&audio, &mut status);
}

fn post_update_audio_system(
    state: Res<State<AppScreen>>,
    mut audio: NonSendMut<AudioEngine>,
    mut status: ResMut<AudioStatus>,
    mut cmds: MessageReader<AudioCommand>,
) {
    for cmd in cmds.read() {
        match cmd {
            AudioCommand::PlayCommon(snd) => audio.play_common(*snd),
            AudioCommand::PlayNnv { stem, phrase_idx } => audio.play_nnv(stem, *phrase_idx),
            AudioCommand::PlayIss { stem, song_idx } => audio.play_iss(stem, *song_idx),
            AudioCommand::PlayUiClick => audio.play_ui_click(),
            AudioCommand::PlayError => audio.play_error(),
            AudioCommand::StopAll => audio.stop_all(),
            AudioCommand::SetEnabled { music, sfx } => audio.set_enabled(*music, *sfx),
            AudioCommand::SetPaused(paused) => audio.set_paused(*paused),
        }
    }
    audio.set_paused(matches!(state.get(), AppScreen::Setup { .. }));
    sync_audio_status(&audio, &mut status);
}

pub fn sync_audio_status(audio: &AudioEngine, status: &mut AudioStatus) {
    status.is_playing = audio.is_playing();
    status.is_effect_playing = audio.is_effect_playing();
    status.common_caption = audio.common_caption().map(str::to_owned);
    status.caption_key = audio.caption_key();
    status.sfx_enabled = audio.sfx_enabled;
    status.music_enabled = audio.music_enabled;
    status.available_nnv = audio.nnv_banks.keys().cloned().collect();
    status.available_iss_stems = audio.iss_songs.keys().map(|(s, _)| s.clone()).collect();
}

impl Plugin for AudioSyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioStatus>()
            .add_systems(PreUpdate, pre_update_audio_system)
            .add_systems(
                PostUpdate,
                post_update_audio_system.after(flush_state_transitions_system),
            );
    }
}

/// Puzzle Browser screen systems.
pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            browser::browser_input_system
                .in_set(GameSet::MenuAndBrowserInput)
                .run_if(in_state(AppScreen::Browser)),
        )
        .add_systems(
            OnExit(AppScreen::Browser),
            browser::clear_browser_pointer_on_exit,
        );
    }
}

/// DOS Setup screen systems.
pub struct SetupPlugin;

impl Plugin for SetupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            setup::setup_input_system
                .in_set(GameSet::MenuAndBrowserInput)
                .run_if(screen::in_setup),
        );
    }
}

/// Active puzzle gameplay systems: in-game menu, timers/animations, piece input, and completion.
pub struct PuzzlePlugin;

impl Plugin for PuzzlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InGameMenuState>()
            .init_resource::<puzzle_time::AnimFinishedThisFrame>()
            .init_resource::<puzzle_completion::CheckCompletionFlag>()
            .add_systems(
                Update,
                menu::in_game_menu_system
                    .in_set(GameSet::MenuAndBrowserInput)
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(
                Update,
                puzzle_time::puzzle_timers_and_animation_system
                    .in_set(GameSet::PuzzleTimersAndAnimation)
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(
                Update,
                puzzle_input::puzzle_input_system
                    .in_set(GameSet::PuzzleInputAndActions)
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(
                Update,
                puzzle_completion::puzzle_completion_system
                    .in_set(GameSet::PuzzleCompletion)
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

/// Tracks authoritative ECS state changes into `RenderVersions` for SDL/software caching.
pub struct RenderTrackingPlugin;

impl Plugin for RenderTrackingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderVersions>().add_systems(
            PostUpdate,
            change_tracking::track_render_changes_system
                .after(flush_state_transitions_system),
        );
    }
}
