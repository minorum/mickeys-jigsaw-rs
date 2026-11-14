//! ECS system sets for deterministic per-frame update ordering.

pub mod browser;
pub mod change_tracking;
pub mod menu;
pub mod puzzle_completion;
pub mod puzzle_input;
pub mod puzzle_time;
pub mod screen;
pub mod setup;

use bevy_ecs::prelude::*;

/// Ordered system sets defining the deterministic per-frame gameplay update sequence.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSet {
    GlobalAndScreenTransitions,
    MenuAndBrowserInput,
    PuzzleTimersAndAnimation,
    PuzzleInputAndActions,
    PuzzleCompletion,
    SyncAndChangeTracking,
}
