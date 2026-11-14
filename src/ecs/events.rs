//! ECS buffered messages for decoupled audio and cross-system commands.

use bevy_ecs::prelude::*;

use crate::audio::CommonSound;

/// Audio commands emitted by gameplay systems and consumed by the audio backend.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum AudioCommand {
    PlayCommon(CommonSound),
    PlayNnv { stem: String, phrase_idx: usize },
    PlayIss { stem: String, song_idx: usize },
    PlayUiClick,
    PlayError,
    StopAll,
    SetEnabled { music: bool, sfx: bool },
    SetPaused(bool),
}
