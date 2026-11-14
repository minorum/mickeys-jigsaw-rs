//! Active puzzle timer, hint countdown, status message expiry, and animation progression system.

use bevy_ecs::prelude::*;
use bevy_state::prelude::State;
use bevy_time::{Real, Time, Virtual};

use crate::app::AppScreen;
use crate::audio::CommonSound;
use crate::ecs::components::{AnimationState, HintHighlight};
use crate::ecs::events::AudioCommand;
use crate::ecs::puzzle_ops::{apply_frame_side_effects, flush_audio_commands};
use crate::ecs::resources::{ActivePuzzle, AudioStatus, InputSnapshot};

/// Marker resource set when an animation finished naturally during this tick's timer update,
/// preventing `Enter` on the same frame from restarting the animation or spawning a keyboard pointer.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct AnimFinishedThisFrame(pub bool);

#[allow(clippy::too_many_arguments)]
pub fn puzzle_timers_and_animation_system(
    mut commands: Commands,
    real_time: Res<Time<Real>>,
    virtual_time: Res<Time<Virtual>>,
    state: Res<State<AppScreen>>,
    input_snap: Res<InputSnapshot>,
    audio_status: Res<AudioStatus>,
    mut active_puzzle: ResMut<ActivePuzzle>,
    mut hint_q: Query<(Entity, &mut HintHighlight)>,
    mut anim_q: Query<(Entity, &mut AnimationState)>,
    mut anim_finished: ResMut<AnimFinishedThisFrame>,
    mut audio_out: MessageWriter<AudioCommand>,
) {
    anim_finished.0 = false;
    if input_snap.consumed_global || input_snap.input.escape || *state.get() != AppScreen::Playing {
        return;
    }
    let Some(meta) = &mut active_puzzle.0 else {
        return;
    };
    let dt = real_time.delta_secs();

    if !meta.solved && meta.timer_enabled && !virtual_time.is_paused() {
        meta.elapsed_secs += virtual_time.delta_secs_f64();
    }

    for (entity, mut hint) in hint_q.iter_mut() {
        hint.timer.tick(real_time.delta());
        if hint.timer.is_finished() {
            commands.entity(entity).remove::<HintHighlight>();
        }
    }

    if let Some((_, ref mut timer)) = meta.status_message {
        timer.tick(real_time.delta());
        if timer.is_finished() {
            meta.status_message = None;
        }
    }

    let mut audio_cmds = Vec::new();
    let mut queued_playing = audio_status.is_playing;
    for (anim_entity, mut anim) in anim_q.iter_mut() {
        anim.tick_accum += dt;
        loop {
            if anim.tick_accum < anim.frame_delay_secs {
                break;
            }
            if anim.wait_for_audio && queued_playing {
                anim.tick_accum = anim.frame_delay_secs;
                break;
            }
            anim.tick_accum -= anim.frame_delay_secs;
            if anim.wait_for_audio {
                anim.bubble = None;
            }
            anim.previous_frame = anim.cur_frame;
            if anim.cur_frame + 1 >= anim.end_frame {
                if anim.repetitions_left <= 1 {
                    if anim.looping_sound.is_some() {
                        audio_cmds.push(AudioCommand::StopAll);
                        queued_playing = false;
                    }
                    if std::mem::take(&mut meta.pending_completion_feedback) {
                        audio_cmds.push(AudioCommand::PlayCommon(CommonSound::DoneWithThisOne));
                        queued_playing = true;
                    }
                    commands.entity(anim_entity).despawn();
                    anim_finished.0 = true;
                    break;
                }
                anim.repetitions_left -= 1;
                anim.cur_frame = anim.start_frame;
            } else {
                anim.cur_frame += 1;
            }

            let mut started_loop_audio = false;
            if !queued_playing && let Some((sound, descriptor)) = anim.looping_sound {
                let desc = &meta.asset.sounds[sound].sounds[descriptor];
                started_loop_audio = true;
                if descriptor == 0 {
                    audio_cmds.push(AudioCommand::PlayIss {
                        stem: desc.stem.clone(),
                        song_idx: desc.phrase_idx as usize,
                    });
                    if audio_status.music_enabled {
                        queued_playing = true;
                    }
                } else {
                    audio_cmds.push(AudioCommand::PlayNnv {
                        stem: desc.stem.clone(),
                        phrase_idx: desc.phrase_idx as usize,
                    });
                    if audio_status.sfx_enabled {
                        queued_playing = true;
                    }
                }
            }
            let frame = anim.cur_frame;
            let mut local_status = audio_status.clone();
            if queued_playing || started_loop_audio {
                local_status.is_playing = true;
            }
            let cmd_start = audio_cmds.len();
            apply_frame_side_effects(
                &mut meta.asset,
                &mut meta.voice_rng,
                meta.text_enabled,
                &local_status,
                &mut anim,
                frame,
                &mut audio_cmds,
            );
            for cmd in &audio_cmds[cmd_start..] {
                match cmd {
                    AudioCommand::StopAll => queued_playing = false,
                    AudioCommand::PlayCommon(_) => queued_playing = true,
                    AudioCommand::PlayNnv { .. } if audio_status.sfx_enabled => {
                        queued_playing = true;
                    }
                    AudioCommand::PlayIss { .. } if audio_status.music_enabled => {
                        queued_playing = true;
                    }
                    _ => {}
                }
            }
        }
    }
    flush_audio_commands(audio_cmds, &mut audio_out);
}
