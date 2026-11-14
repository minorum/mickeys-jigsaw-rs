//! Active puzzle gameplay input, tray/board/group interactions, hotkeys, and hotspot clicks.

use bevy_ecs::prelude::*;
use bevy_state::prelude::{NextState, State};

use crate::app::AppScreen;
use crate::audio::CommonSound;
use crate::ecs::components::{
    AnimationState, HintHighlight, PieceEdges, PieceGeometry, PieceIdentity, PieceLocation,
    SceneHotspot,
};
use crate::ecs::events::AudioCommand;
use crate::ecs::puzzle_ops::{
    build_animation_state, can_place_piece_at, complete_puzzle, default_playback,
    flush_audio_commands, solved_sequence_at,
};
use crate::ecs::resources::{
    ActivePuzzle, AudioStatus, GameConfig, InGameMenuState, InputSnapshot, PuzzleMeta,
};
use crate::ecs::systems::menu::{
    move_keyboard_pointer_meta, request_quit_from_puzzle, return_held_to_tray_at, return_to_browser,
};
use crate::ecs::systems::puzzle_completion::CheckCompletionFlag;
use crate::ecs::systems::puzzle_time::AnimFinishedThisFrame;
use crate::graphics::SCENE_OFFSET_X;
use crate::puzzle::{TOOLBOX_W, export_print_pbm};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn puzzle_input_system(
    mut commands: Commands,
    state: Res<State<AppScreen>>,
    mut next_state: ResMut<NextState<AppScreen>>,
    input_snap: Res<InputSnapshot>,
    mut config: ResMut<GameConfig>,
    mut menu: ResMut<InGameMenuState>,
    audio_status: Res<AudioStatus>,
    anim_finished: Res<AnimFinishedThisFrame>,
    mut check_completion: ResMut<CheckCompletionFlag>,
    mut active_puzzle: ResMut<ActivePuzzle>,
    mut piece_q: Query<(
        Entity,
        &PieceIdentity,
        &PieceGeometry,
        &PieceEdges,
        &mut PieceLocation,
    )>,
    mut anim_q: Query<Entity, With<AnimationState>>,
    hint_q: Query<Entity, With<HintHighlight>>,
    hotspot_q: Query<&SceneHotspot>,
    mut audio_out: MessageWriter<AudioCommand>,
) {
    let cur_screen = *state.get();
    if input_snap.consumed_global || cur_screen != AppScreen::Playing {
        return;
    }
    let input = input_snap.input;
    let dt = input_snap.dt;

    if input.escape {
        let mut temp_q = piece_q.transmute_lens::<(&PieceIdentity, &mut PieceLocation)>();
        return_to_browser(
            cur_screen,
            &mut next_state,
            &mut menu,
            &mut active_puzzle,
            &temp_q.query(),
            &mut audio_out,
        );
        return;
    }
    if input.key_q {
        request_quit_from_puzzle(&mut menu, &mut active_puzzle, &mut audio_out);
        return;
    }

    let Some(meta) = &mut active_puzzle.0 else {
        return;
    };

    let mut open_menu = false;

    if input.right_pressed
        || (input.pointer_moved && input.move_x == 0 && input.move_y == 0 && !input.enter)
    {
        meta.keyboard_pointer = None;
    }

    if !input.left_pressed && !input.right_pressed {
        if input.move_x != 0 || input.move_y != 0 {
            move_keyboard_pointer_meta(meta, input.move_x as f32, input.move_y as f32, dt);
        }
        if input.enter && !anim_finished.0 {
            meta.drag_origin = None;
            let has_anim = anim_q.iter().next().is_some();
            if has_anim {
                if handle_puzzle_click(
                    0,
                    0,
                    meta,
                    &mut commands,
                    &audio_status,
                    &mut check_completion,
                    &mut piece_q,
                    &mut anim_q,
                    &hint_q,
                    &hotspot_q,
                    &mut audio_out,
                ) {
                    open_menu = true;
                }
            } else {
                let (x, y) = *meta.keyboard_pointer.get_or_insert((20.0, 40.0));
                if handle_puzzle_click(
                    x.round() as i32,
                    y.round() as i32,
                    meta,
                    &mut commands,
                    &audio_status,
                    &mut check_completion,
                    &mut piece_q,
                    &mut anim_q,
                    &hint_q,
                    &hotspot_q,
                    &mut audio_out,
                ) {
                    open_menu = true;
                }
            }
        }
    }

    if input.right_pressed && !meta.solved {
        if !handle_right_click(
            meta,
            &mut piece_q,
            &mut audio_out,
            input.pointer_x,
            input.pointer_y,
        ) {
            toggle_toolbox(meta);
        }
    } else if input.space && !meta.solved {
        toggle_toolbox(meta);
    }

    if input.f1 && trigger_hint(meta, &mut commands, &piece_q, &hint_q) {
        audio_out.write(AudioCommand::PlayCommon(CommonSound::SeeWhatHappens));
    }

    if input.f2 && !meta.solved {
        for (_, ident, _, _, mut loc) in piece_q.iter_mut() {
            *loc = PieceLocation::Board {
                row: ident.home_row,
                col: ident.home_col,
            };
        }
        meta.tray_scroll = 0;
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
    }

    if input.f3 || input.key_g {
        meta.grid_lines = !meta.grid_lines;
        config.prefs.grid_lines_on = u16::from(meta.grid_lines);
    }

    if input.f5 || input.key_p {
        let msg = match export_print_pbm(&config.game_dir, meta.puzzle_num) {
            Ok(ok_msg) => ok_msg,
            Err(err_msg) => err_msg,
        };
        meta.set_status_message(msg, 3.0);
    }

    if input.f7 {
        config.prefs.music_on = u16::from(config.prefs.music_on == 0);
        audio_out.write(AudioCommand::SetEnabled {
            music: config.prefs.music_on != 0,
            sfx: config.prefs.sfx_on != 0,
        });
        audio_out.write(AudioCommand::PlayCommon(CommonSound::UiClick));
        meta.set_status_message(
            if config.prefs.music_on != 0 {
                "Music: ON"
            } else {
                "Music: OFF"
            },
            2.0,
        );
    }

    if input.f8 {
        config.prefs.sfx_on = u16::from(config.prefs.sfx_on == 0);
        audio_out.write(AudioCommand::SetEnabled {
            music: config.prefs.music_on != 0,
            sfx: config.prefs.sfx_on != 0,
        });
        audio_out.write(AudioCommand::PlayCommon(CommonSound::UiClick));
        meta.set_status_message(
            if config.prefs.sfx_on != 0 {
                "Sound Effects: ON"
            } else {
                "Sound Effects: OFF"
            },
            2.0,
        );
    }

    if input.key_t {
        config.prefs.timer_on = u16::from(config.prefs.timer_on == 0);
        meta.timer_enabled = config.prefs.timer_on != 0;
        audio_out.write(AudioCommand::PlayCommon(CommonSound::UiClick));
        meta.set_status_message(
            if config.prefs.timer_on != 0 {
                "Timer: ON"
            } else {
                "Timer: OFF"
            },
            2.0,
        );
    }

    if input.key_m {
        config.prefs.text_msg_on = u16::from(config.prefs.text_msg_on == 0);
        meta.text_enabled = config.prefs.text_msg_on != 0;
        audio_out.write(AudioCommand::PlayCommon(CommonSound::UiClick));
        meta.set_status_message(
            if config.prefs.text_msg_on != 0 {
                "Text Messages: ON"
            } else {
                "Text Messages: OFF"
            },
            2.0,
        );
    }

    if input.key_r && meta.solved {
        meta.pending_completion_feedback = false;
        audio_out.write(AudioCommand::StopAll);
        audio_out.write(AudioCommand::PlayCommon(CommonSound::SeeWhatHappens));
        for e in anim_q.iter() {
            commands.entity(e).despawn();
        }
        let playback = default_playback(&meta.asset);
        let mut cmds = Vec::new();
        if let Some(anim) = build_animation_state(meta, playback, &audio_status, &mut cmds) {
            flush_audio_commands(cmds, &mut audio_out);
            commands.spawn(anim);
        }
    }

    let tray_len = piece_q
        .iter()
        .filter(|(_, _, _, _, loc)| loc.is_tray())
        .count();
    if input.up || input.wheel_delta > 0 {
        scroll_tray(meta, -1, tray_len);
    }
    if input.down || input.wheel_delta < 0 {
        scroll_tray(meta, 1, tray_len);
    }

    if input.left_pressed {
        meta.keyboard_pointer = None;
        meta.drag_origin = None;
        let has_anim = anim_q.iter().next().is_some();
        let tray_len = piece_q
            .iter()
            .filter(|(_, _, _, _, loc)| loc.is_tray())
            .count();
        let pressed = meta.toolbox_target_at(input.pointer_x, input.pointer_y, has_anim, tray_len);
        let was_holding = piece_q.iter().any(|(_, _, _, _, loc)| loc.is_held());
        let menu_clicked = handle_puzzle_click(
            input.pointer_x,
            input.pointer_y,
            meta,
            &mut commands,
            &audio_status,
            &mut check_completion,
            &mut piece_q,
            &mut anim_q,
            &hint_q,
            &hotspot_q,
            &mut audio_out,
        );
        if menu_clicked {
            open_menu = true;
            meta.pressed_toolbox_target = None;
        } else {
            meta.pressed_toolbox_target = pressed;
        }
        let now_holding = piece_q.iter().any(|(_, _, _, _, loc)| loc.is_held());
        if !was_holding && now_holding {
            meta.drag_origin = Some((input.pointer_x, input.pointer_y));
        }
    }

    if input.left_released {
        meta.pressed_toolbox_target = None;
        if let Some((start_x, start_y)) = meta.drag_origin.take() {
            let is_holding = piece_q.iter().any(|(_, _, _, _, loc)| loc.is_held());
            if (0..320).contains(&input.pointer_x)
                && (0..200).contains(&input.pointer_y)
                && is_holding
                && (input.pointer_x - start_x)
                    .abs()
                    .max((input.pointer_y - start_y).abs())
                    >= 4
                && !(meta.toolbox_open
                    && input.pointer_x < TOOLBOX_W
                    && !(30..186).contains(&input.pointer_y))
            {
                handle_puzzle_click(
                    input.pointer_x,
                    input.pointer_y,
                    meta,
                    &mut commands,
                    &audio_status,
                    &mut check_completion,
                    &mut piece_q,
                    &mut anim_q,
                    &hint_q,
                    &hotspot_q,
                    &mut audio_out,
                );
            }
        }
    }

    if open_menu {
        meta.drag_origin = None;
        meta.pressed_toolbox_target = None;
        menu.open = true;
        menu.selected = 0;
        menu.pressed = None;
        audio_out.write(AudioCommand::PlayUiClick);
    }
}

fn toggle_toolbox(meta: &mut PuzzleMeta) {
    if meta.solved {
        return;
    }
    meta.drag_origin = None;
    meta.pressed_toolbox_target = None;
    meta.toolbox_open = !meta.toolbox_open;
}

fn scroll_tray(meta: &mut PuzzleMeta, delta: i32, tray_len: usize) -> bool {
    if !meta.toolbox_open || meta.solved {
        return false;
    }
    let prev = meta.tray_scroll;
    if delta < 0 {
        meta.tray_scroll = meta
            .tray_scroll
            .saturating_sub(delta.unsigned_abs() as usize);
    } else if delta > 0 {
        meta.tray_scroll = meta.tray_scroll.saturating_add(delta as usize);
        meta.clamp_tray_scroll(tray_len);
    }
    meta.tray_scroll != prev
}

fn trigger_hint(
    meta: &mut PuzzleMeta,
    commands: &mut Commands,
    piece_q: &Query<(
        Entity,
        &PieceIdentity,
        &PieceGeometry,
        &PieceEdges,
        &mut PieceLocation,
    )>,
    hint_q: &Query<Entity, With<HintHighlight>>,
) -> bool {
    if meta.solved {
        return false;
    }
    // Priority: held anchor -> first visible tray piece -> first tray piece -> first misplaced board piece
    let held_anchor = piece_q.iter().find_map(|(e, _, _, _, loc)| {
        matches!(
            *loc,
            PieceLocation::Held {
                is_anchor: true,
                ..
            }
        )
        .then_some(e)
    });
    let target_entity = held_anchor
        .or_else(|| {
            piece_q.iter().find_map(|(e, _, _, _, loc)| {
                matches!(*loc, PieceLocation::Tray { order } if order == meta.tray_scroll)
                    .then_some(e)
            })
        })
        .or_else(|| {
            piece_q
                .iter()
                .filter_map(|(e, _, _, _, loc)| match *loc {
                    PieceLocation::Tray { order } => Some((order, e)),
                    _ => None,
                })
                .min_by_key(|&(order, _)| order)
                .map(|(_, e)| e)
        })
        .or_else(|| {
            if meta.grid_dim <= 1 {
                return None;
            }
            piece_q
                .iter()
                .filter_map(|(e, ident, _, _, loc)| match *loc {
                    PieceLocation::Board { row, col }
                        if row != ident.home_row || col != ident.home_col =>
                    {
                        Some((row * meta.grid_dim + col, e))
                    }
                    _ => None,
                })
                .min_by_key(|&(cell, _)| cell)
                .map(|(_, e)| e)
        });

    if let Some(target) = target_entity {
        for e in hint_q.iter() {
            if e != target {
                commands.entity(e).remove::<HintHighlight>();
            }
        }
        commands.entity(target).insert(HintHighlight::new(1.5));
        true
    } else {
        false
    }
}

fn handle_right_click(
    meta: &mut PuzzleMeta,
    piece_q: &mut Query<(
        Entity,
        &PieceIdentity,
        &PieceGeometry,
        &PieceEdges,
        &mut PieceLocation,
    )>,
    audio_out: &mut MessageWriter<AudioCommand>,
    mx: i32,
    my: i32,
) -> bool {
    if meta.solved || meta.grid_dim <= 1 || piece_q.iter().any(|(_, _, _, _, loc)| loc.is_held()) {
        return false;
    }
    if meta.toolbox_open && mx < TOOLBOX_W {
        return false;
    }
    let pfx = meta.playfield_x();
    let pfy = meta.playfield_y();
    let Some((start_r, start_c)) = meta.grid_cell_at(mx - pfx, my - pfy) else {
        return false;
    };
    let dim = meta.grid_dim;
    let mut grid = vec![None; dim * dim];
    for (e, _, _, _, loc) in piece_q.iter() {
        if let PieceLocation::Board { row, col } = *loc {
            grid[row * dim + col] = Some(e);
        }
    }
    let Some(anchor_entity) = grid[start_r * dim + start_c] else {
        return false;
    };

    // Flood-fill connected placed pieces (`FUN_1e59_125f`)
    let mut visited = vec![false; dim * dim];
    let mut queue = vec![(start_r, start_c)];
    visited[start_r * dim + start_c] = true;
    let mut group: Vec<(Entity, i32, i32)> = Vec::new();
    let mut head = 0;
    while head < queue.len() {
        let (r, c) = queue[head];
        head += 1;
        if let Some(e) = grid[r * dim + c] {
            group.push((e, r as i32 - start_r as i32, c as i32 - start_c as i32));
            let neighbors = [
                (r > 0).then(|| (r - 1, c)),
                (r + 1 < dim).then(|| (r + 1, c)),
                (c > 0).then(|| (r, c - 1)),
                (c + 1 < dim).then(|| (r, c + 1)),
            ];
            for (nr, nc) in neighbors.into_iter().flatten() {
                let nidx = nr * dim + nc;
                if !visited[nidx] && grid[nidx].is_some() {
                    visited[nidx] = true;
                    queue.push((nr, nc));
                }
            }
        }
    }

    for (order, (e, dr, dc)) in group.into_iter().enumerate() {
        if let Ok((_, _, _, _, mut loc)) = piece_q.get_mut(e) {
            *loc = PieceLocation::Held {
                is_anchor: e == anchor_entity,
                group_offset: (dr, dc),
                group_order: order,
            };
        }
    }
    meta.held_group_origin_cell = Some((start_r, start_c));
    audio_out.write(AudioCommand::PlayCommon(CommonSound::ChoosePiece));
    true
}

/// Returns `true` if the click opened the in-game menu.
#[allow(clippy::too_many_arguments)]
fn handle_puzzle_click(
    mx: i32,
    my: i32,
    meta: &mut PuzzleMeta,
    commands: &mut Commands,
    audio_status: &AudioStatus,
    check_completion: &mut CheckCompletionFlag,
    piece_q: &mut Query<(
        Entity,
        &PieceIdentity,
        &PieceGeometry,
        &PieceEdges,
        &mut PieceLocation,
    )>,
    anim_q: &mut Query<Entity, With<AnimationState>>,
    hint_q: &Query<Entity, With<HintHighlight>>,
    hotspot_q: &Query<&SceneHotspot>,
    audio_out: &mut MessageWriter<AudioCommand>,
) -> bool {
    if !(0..320).contains(&mx) || !(0..200).contains(&my) {
        return false;
    }

    // 1. If an animation is playing, clicking skips to the end
    let has_anim = anim_q.iter().next().is_some();
    if has_anim {
        for e in anim_q.iter() {
            commands.entity(e).despawn();
        }
        meta.pending_completion_feedback = false;
        audio_out.write(AudioCommand::StopAll);
        return false;
    }

    // 2. If solved, check menu corner or interactive hotspots
    if meta.solved {
        if mx < 28 && my < 24 {
            return true;
        }
        let playback = solved_sequence_at(&meta.asset, hotspot_q.iter(), mx - SCENE_OFFSET_X, my);
        let mut cmds = Vec::new();
        if let Some(anim) = build_animation_state(meta, playback, audio_status, &mut cmds) {
            flush_audio_commands(cmds, audio_out);
            commands.spawn(anim);
        } else {
            flush_audio_commands(cmds, audio_out);
        }
        return false;
    }

    // 3. Toolbox & Tray interactions
    if meta.toolbox_open && mx < TOOLBOX_W {
        if my < 18 {
            return true;
        }
        let tray_len = piece_q
            .iter()
            .filter(|(_, _, _, _, loc)| loc.is_tray())
            .count();
        if my < 30 {
            if scroll_tray(meta, -1, tray_len) {
                audio_out.write(AudioCommand::PlayUiClick);
            }
            return false;
        }
        if my >= 186 {
            if scroll_tray(meta, 1, tray_len) {
                audio_out.write(AudioCommand::PlayUiClick);
            }
            return false;
        }
        let vis = meta.visible_tray_slots();
        let slot_h = meta.tray_slot_height();
        let slot = (((my - 30) / slot_h.max(1)) as usize).min(vis - 1);
        let tray_pos = meta.tray_scroll + slot;
        let is_holding = piece_q.iter().any(|(_, _, _, _, loc)| loc.is_held());
        if is_holding {
            let mut lens = piece_q.transmute_lens::<(&PieceIdentity, &mut PieceLocation)>();
            return_held_to_tray_at(tray_pos, meta, &mut lens.query(), audio_out);
        } else if tray_pos < tray_len {
            let mut picked = None;
            for (e, _, _, _, loc) in piece_q.iter() {
                if *loc == (PieceLocation::Tray { order: tray_pos }) {
                    picked = Some(e);
                    break;
                }
            }
            if let Some(picked_e) = picked {
                if let Ok((_, _, _, _, mut loc)) = piece_q.get_mut(picked_e) {
                    *loc = PieceLocation::Held {
                        is_anchor: true,
                        group_offset: (0, 0),
                        group_order: 0,
                    };
                }
                for (_, _, _, _, mut loc) in piece_q.iter_mut() {
                    if let PieceLocation::Tray { order } = *loc
                        && order > tray_pos
                    {
                        *loc = PieceLocation::Tray { order: order - 1 };
                    }
                }
                meta.held_group_origin_cell = None;
                let new_tray_len = tray_len - 1;
                if meta.tray_scroll >= new_tray_len {
                    meta.clamp_tray_scroll(new_tray_len);
                }
                audio_out.write(AudioCommand::PlayCommon(CommonSound::ChoosePiece));
            }
        }
        return false;
    }

    if !meta.toolbox_open && (0..18).contains(&mx) && (0..16).contains(&my) {
        meta.toolbox_open = true;
        return false;
    }

    // 4. Playfield interactions
    let pfx = meta.playfield_x();
    let pfy = meta.playfield_y();
    let held_list: Vec<(Entity, PieceGeometry, PieceEdges, (i32, i32))> = piece_q
        .iter()
        .filter_map(|(e, _, geom, edges, loc)| match *loc {
            PieceLocation::Held { group_offset, .. } => Some((e, *geom, *edges, group_offset)),
            _ => None,
        })
        .collect();

    if !held_list.is_empty() {
        if meta.grid_dim > 1 {
            let px = mx - pfx;
            let py = my - pfy;
            if let Some((target_r, target_c)) = meta.grid_cell_at(px, py) {
                let dim = meta.grid_dim;
                let dim_i32 = dim as i32;
                let mut placements = Vec::with_capacity(held_list.len());
                let mut all_valid = true;
                for &(e, _, ref edges, (dr, dc)) in &held_list {
                    let nr = target_r as i32 + dr;
                    let nc = target_c as i32 + dc;
                    if !(0..dim_i32).contains(&nr) || !(0..dim_i32).contains(&nc) {
                        all_valid = false;
                        break;
                    }
                    let ur = nr as usize;
                    let uc = nc as usize;
                    let board_iter = piece_q.iter().filter_map(|(_, _, _, pe, loc)| match *loc {
                        PieceLocation::Board { row, col } => Some((row, col, pe)),
                        _ => None,
                    });
                    if !can_place_piece_at(dim, edges, ur, uc, board_iter) {
                        all_valid = false;
                        break;
                    }
                    placements.push((e, ur, uc));
                }
                if all_valid {
                    for (e, ur, uc) in placements {
                        if let Ok((_, _, _, _, mut loc)) = piece_q.get_mut(e) {
                            *loc = PieceLocation::Board { row: ur, col: uc };
                        }
                    }
                    meta.held_group_origin_cell = None;
                    for e in hint_q.iter() {
                        commands.entity(e).remove::<HintHighlight>();
                    }
                    check_completion.0 = true;
                    return false;
                }
            }
            audio_out.write(AudioCommand::PlayError);
        } else {
            // Preset puzzle (`grid_dim == 1`)
            let (e, geom, _, _) = held_list[0];
            let target_cx = pfx + geom.base_x + geom.base_w / 2;
            let target_cy = pfy + geom.base_y + geom.base_h / 2;
            let snap_tol_x = (geom.base_w / 2).max(12);
            let snap_tol_y = (geom.base_h / 2).max(12);
            if (mx - target_cx).abs() <= snap_tol_x && (my - target_cy).abs() <= snap_tol_y {
                if let Ok((_, ident, _, _, mut loc)) = piece_q.get_mut(e) {
                    *loc = PieceLocation::Board {
                        row: ident.home_row,
                        col: ident.home_col,
                    };
                }
                for he in hint_q.iter() {
                    commands.entity(he).remove::<HintHighlight>();
                }
                check_completion.0 = true;
            } else {
                audio_out.write(AudioCommand::PlayError);
            }
        }
    } else if meta.grid_dim > 1
        && let Some((r, c)) = meta.grid_cell_at(mx - pfx, my - pfy)
    {
        // Pick up a placed piece from the board (`FUN_1e59_14b6`)
        let target_board = piece_q.iter().find_map(|(e, _, _, _, loc)| {
            (*loc == PieceLocation::Board { row: r, col: c }).then_some(e)
        });
        if let Some(e) = target_board {
            if let Ok((_, _, _, _, mut loc)) = piece_q.get_mut(e) {
                *loc = PieceLocation::Held {
                    is_anchor: true,
                    group_offset: (0, 0),
                    group_order: 0,
                };
            }
            meta.held_group_origin_cell = None;
            audio_out.write(AudioCommand::PlayCommon(CommonSound::ChoosePiece));
        }
    }

    false
}

/// Cancels an active mouse/touch drag when interrupted (e.g., holding Tab for reference).
pub fn cancel_pointer_drag(world: &mut World) {
    if let Some(meta) = &mut world.resource_mut::<ActivePuzzle>().0 {
        meta.drag_origin = None;
        meta.pressed_toolbox_target = None;
    }
}
