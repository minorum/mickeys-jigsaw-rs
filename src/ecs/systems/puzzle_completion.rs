//! Puzzle completion detection system.

use bevy_ecs::prelude::*;
use bevy_state::prelude::State;

use crate::app::AppScreen;
use crate::audio::CommonSound;
use crate::ecs::components::{AnimationState, HintHighlight, PieceIdentity, PieceLocation};
use crate::ecs::events::AudioCommand;
use crate::ecs::puzzle_ops::complete_puzzle;
use crate::ecs::resources::{ActivePuzzle, AudioStatus, InputSnapshot};

/// Marker resource set when a piece placement occurred this frame and should be checked for completion.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CheckCompletionFlag(pub bool);

#[allow(clippy::too_many_arguments)]
pub fn puzzle_completion_system(
    mut commands: Commands,
    state: Res<State<AppScreen>>,
    input_snap: Res<InputSnapshot>,
    audio_status: Res<AudioStatus>,
    mut active_puzzle: ResMut<ActivePuzzle>,
    mut check_flag: ResMut<CheckCompletionFlag>,
    piece_q: Query<(&PieceIdentity, &PieceLocation)>,
    anim_q: Query<Entity, With<AnimationState>>,
    hint_q: Query<Entity, With<HintHighlight>>,
    mut audio_out: MessageWriter<AudioCommand>,
) {
    if !std::mem::take(&mut check_flag.0) {
        return;
    }
    if input_snap.consumed_global || *state.get() != AppScreen::Playing {
        return;
    }
    let Some(meta) = &mut active_puzzle.0 else {
        return;
    };
    if meta.solved {
        return;
    }

    let mut total = 0usize;
    let mut all_placed = true;
    let mut all_home = true;
    for (ident, loc) in piece_q.iter() {
        total += 1;
        match *loc {
            PieceLocation::Board { row, col } => {
                if meta.grid_dim > 1 && (row != ident.home_row || col != ident.home_col) {
                    all_home = false;
                }
            }
            _ => {
                all_placed = false;
                all_home = false;
            }
        }
    }

    if total == 0 || !all_placed {
        return;
    }

    if all_home {
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
    } else if meta.grid_dim > 1 {
        audio_out.write(AudioCommand::PlayCommon(CommonSound::NotDoneYet));
        meta.set_status_message("Not finished yet. Push F1 for a hint or F2 to finish.", 5.0);
    }
}
