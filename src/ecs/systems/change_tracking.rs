//! Automatic render-revision tracking using `bevy_ecs` queries and change detection.

use bevy_ecs::prelude::*;
use bevy_state::prelude::State;

use crate::app::AppScreen;
use crate::ecs::components::{AnimationState, HintHighlight, PieceIdentity, PieceLocation};
use crate::ecs::resources::{
    ActivePuzzle, BrowserState, GameConfig, InGameMenuState, RenderVersions, ScreenState,
    SetupState,
};

/// Tracks authoritative ECS state changes and updates `RenderVersions`
/// (`app_rev`, `board_rev`, `tray_rev`, `visual_rev`) without manual `bump_*` calls.
#[allow(clippy::too_many_arguments)]
pub fn track_render_changes_system(
    mut versions: ResMut<RenderVersions>,
    state: Res<State<AppScreen>>,
    screen_state: Res<ScreenState>,
    browser: Res<BrowserState>,
    setup: Res<SetupState>,
    menu: Res<InGameMenuState>,
    config: Res<GameConfig>,
    active_puzzle: Res<ActivePuzzle>,
    piece_q: Query<(&PieceIdentity, &PieceLocation)>,
    hint_q: Query<(Entity, &HintHighlight)>,
    anim_q: Query<&AnimationState>,
    mut last_board_hash: Local<u64>,
    mut last_tray_hash: Local<u64>,
    mut last_extra_vis_hash: Local<u64>,
    mut last_screen_error_hash: Local<Option<(bool, Option<String>)>>,
) {
    let screen_sig = (screen_state.quit_requested, screen_state.error.clone());
    let screen_meta_changed = last_screen_error_hash.as_ref() != Some(&screen_sig);
    if screen_meta_changed {
        *last_screen_error_hash = Some(screen_sig);
    }
    if state.is_changed()
        || screen_meta_changed
        || setup.is_changed()
        || menu.is_changed()
        || config.is_changed()
        || browser.is_changed()
    {
        versions.app_rev = versions.app_rev.wrapping_add(1);
    }

    let Some(meta) = &active_puzzle.0 else {
        versions.last_session_id = None;
        return;
    };

    // Compute exact hashes of board occupancy, tray ordering, and visual overlays
    let mut board_hash = 0xcbf2_9ce4_8422_2325u64;
    let mut tray_hash = 0xcbf2_9ce4_8422_2325u64;
    for (ident, loc) in piece_q.iter() {
        match *loc {
            PieceLocation::Board { row, col } => {
                let v = (((ident.index as u64 + 1) << 16)
                    | ((row as u64 + 1) << 8)
                    | (col as u64 + 1))
                    ^ 0x9e37_79b9_7f4a_7c15;
                board_hash ^= v.wrapping_mul(0x1000_0000_01b3);
            }
            PieceLocation::Tray { order } => {
                let v = (((ident.index as u64 + 1) << 16) | (order as u64 + 1))
                    ^ 0x9e37_79b9_7f4a_7c15;
                tray_hash ^= v.wrapping_mul(0x1000_0000_01b3);
            }
            PieceLocation::Held { .. } => {}
        }
    }

    let session_changed = versions.last_session_id != Some(meta.session_id);
    let board_meta_changed = session_changed
        || versions.last_toolbox_open != Some(meta.toolbox_open)
        || versions.last_grid_lines != Some(meta.grid_lines)
        || versions.last_solved != Some(meta.solved);

    let board_changed = board_meta_changed || *last_board_hash != board_hash;
    let tray_changed = session_changed
        || versions.last_tray_scroll != Some(meta.tray_scroll)
        || *last_tray_hash != tray_hash;

    let hint_phase = hint_q
        .iter()
        .next()
        .map(|(e, h)| (e, ((h.timer.remaining_secs() * 8.0) as i32) & 1));
    let anim_vis = anim_q
        .iter()
        .next()
        .map(|a| (a.cur_frame, a.bubble.filter(|_| meta.text_enabled)));

    // Hash status message presence/text and keyboard pointer position
    let mut extra_vis_hash = 0xcbf2_9ce4_8422_2325u64;
    if let Some((ref msg, _)) = meta.status_message {
        for &b in msg.as_bytes() {
            extra_vis_hash ^= u64::from(b);
            extra_vis_hash = extra_vis_hash.wrapping_mul(0x1000_0000_01b3);
        }
    }
    if let Some((kx, ky)) = meta.keyboard_pointer {
        extra_vis_hash ^= (kx.to_bits() as u64) ^ ((ky.to_bits() as u64) << 32);
    }

    let vis_changed = board_changed
        || tray_changed
        || versions.last_hint_phase != hint_phase
        || versions.last_anim_vis != anim_vis
        || *last_extra_vis_hash != extra_vis_hash;

    if board_changed {
        versions.board_rev = versions.board_rev.wrapping_add(1);
    }
    if tray_changed {
        versions.tray_rev = versions.tray_rev.wrapping_add(1);
    }
    if vis_changed {
        versions.visual_rev = versions.visual_rev.wrapping_add(1);
    }

    versions.last_session_id = Some(meta.session_id);
    versions.last_toolbox_open = Some(meta.toolbox_open);
    versions.last_grid_lines = Some(meta.grid_lines);
    versions.last_solved = Some(meta.solved);
    versions.last_tray_scroll = Some(meta.tray_scroll);
    versions.last_hint_phase = hint_phase;
    versions.last_anim_vis = anim_vis;
    *last_board_hash = board_hash;
    *last_tray_hash = tray_hash;
    *last_extra_vis_hash = extra_vis_hash;
}
