//! DOS Setup screen input system and atomic VGA/EGA video mode switching.

use bevy_ecs::prelude::*;
use bevy_state::prelude::{NextState, State};

use crate::app::AppScreen;
use crate::assets::PuzzleAsset;
use crate::audio::CommonSound;
use crate::ecs::components::{AnimationState, PieceIdentity, PieceSprite, SceneHotspot};
use crate::ecs::events::AudioCommand;
use crate::ecs::puzzle_ops::{cut_pieces_data, extract_hotspots, render_full_picture_into};
use crate::ecs::resources::{
    ActivePuzzle, BrowserState, GameConfig, InputSnapshot, ScreenState, SetupState,
};
use crate::ecs::systems::screen::set_screen;

#[allow(clippy::too_many_arguments)]
pub fn setup_input_system(
    mut commands: Commands,
    state: Res<State<AppScreen>>,
    mut next_state: ResMut<NextState<AppScreen>>,
    input_snap: Res<InputSnapshot>,
    mut screen_state: ResMut<ScreenState>,
    mut setup: ResMut<SetupState>,
    mut config: ResMut<GameConfig>,
    mut browser: ResMut<BrowserState>,
    mut active_puzzle: ResMut<ActivePuzzle>,
    mut piece_q: Query<(
        Entity,
        &PieceIdentity,
        &mut crate::ecs::components::PieceGeometry,
        &mut PieceSprite,
    )>,
    hotspot_q: Query<Entity, With<SceneHotspot>>,
    anim_q: Query<Entity, With<AnimationState>>,
    mut audio_out: MessageWriter<AudioCommand>,
) {
    if input_snap.consumed_global {
        return;
    }
    let cur_screen = *state.get();
    let AppScreen::Setup { return_to_playing } = cur_screen else {
        return;
    };
    let input = input_snap.input;

    if input.up {
        setup.menu.move_up();
        return;
    }
    if input.down {
        setup.menu.move_down();
        return;
    }
    if input.escape {
        exit_setup(
            return_to_playing,
            cur_screen,
            &mut next_state,
            &mut commands,
            &mut screen_state,
            &mut setup,
            &mut config,
            &mut browser,
            &mut active_puzzle,
            &mut piece_q,
            &hotspot_q,
            &anim_q,
            &mut audio_out,
        );
        return;
    }

    let done = if input.left {
        if setup.menu.selected_row + 1 < setup.menu.num_rows() {
            setup.menu.activate_or_cycle(&config.game_dir);
        }
        false
    } else if input.right || input.enter || input.space {
        setup.menu.activate_or_cycle(&config.game_dir)
    } else if input.left_pressed {
        setup
            .menu
            .handle_click(input.pointer_x, input.pointer_y, &config.game_dir)
    } else {
        return;
    };

    if done {
        exit_setup(
            return_to_playing,
            cur_screen,
            &mut next_state,
            &mut commands,
            &mut screen_state,
            &mut setup,
            &mut config,
            &mut browser,
            &mut active_puzzle,
            &mut piece_q,
            &hotspot_q,
            &anim_q,
            &mut audio_out,
        );
    } else {
        sync_prefs_from_setup(
            &mut commands,
            &mut screen_state,
            &mut setup,
            &mut config,
            &mut browser,
            &mut active_puzzle,
            &mut piece_q,
            &hotspot_q,
            &anim_q,
            &mut audio_out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn exit_setup(
    return_to_playing: bool,
    cur_screen: AppScreen,
    next_state: &mut NextState<AppScreen>,
    commands: &mut Commands,
    screen_state: &mut ScreenState,
    setup: &mut SetupState,
    config: &mut GameConfig,
    browser: &mut BrowserState,
    active_puzzle: &mut ActivePuzzle,
    piece_q: &mut Query<(
        Entity,
        &PieceIdentity,
        &mut crate::ecs::components::PieceGeometry,
        &mut PieceSprite,
    )>,
    hotspot_q: &Query<Entity, With<SceneHotspot>>,
    anim_q: &Query<Entity, With<AnimationState>>,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    sync_prefs_from_setup(
        commands,
        screen_state,
        setup,
        config,
        browser,
        active_puzzle,
        piece_q,
        hotspot_q,
        anim_q,
        audio_out,
    );
    audio_out.write(AudioCommand::PlayCommon(CommonSound::SetupStart));
    let next = if return_to_playing && active_puzzle.0.is_some() {
        AppScreen::Playing
    } else {
        AppScreen::Browser
    };
    set_screen(cur_screen, next_state, next, audio_out);
}

#[allow(clippy::too_many_arguments)]
pub fn sync_prefs_from_setup(
    commands: &mut Commands,
    screen_state: &mut ScreenState,
    setup: &mut SetupState,
    config: &mut GameConfig,
    browser: &mut BrowserState,
    active_puzzle: &mut ActivePuzzle,
    piece_q: &mut Query<(
        Entity,
        &PieceIdentity,
        &mut crate::ecs::components::PieceGeometry,
        &mut PieceSprite,
    )>,
    hotspot_q: &Query<Entity, With<SceneHotspot>>,
    anim_q: &Query<Entity, With<AnimationState>>,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    let previous_ega = config.prefs.video_mode >= 2;
    let next_ega = setup.menu.prefs.video_mode >= 2;
    let previous_video = config.prefs.video_mode;

    if previous_ega != next_ega {
        let loaded = PuzzleAsset::load(&config.game_dir, browser.selected_puzzle, next_ega)
            .and_then(|preview| {
                let active = active_puzzle
                    .0
                    .as_ref()
                    .map(|meta| PuzzleAsset::load(&config.game_dir, meta.puzzle_num, next_ega))
                    .transpose()?;
                Ok((preview, active))
            });
        match loaded {
            Ok((preview, active)) => {
                browser.preview_asset = preview;
                if let (Some(meta), Some(asset)) = (&mut active_puzzle.0, active) {
                    replace_scene_asset_ecs(commands, meta, asset, piece_q, hotspot_q, anim_q);
                    audio_out.write(AudioCommand::StopAll);
                }
                config.session_use_ega = next_ega;
                screen_state.error = None;
            }
            Err(error) => {
                setup.menu.set_video_mode(previous_video);
                screen_state.error = Some(error);
            }
        }
    }

    config.prefs = setup.menu.prefs;
    audio_out.write(AudioCommand::SetEnabled {
        music: config.prefs.music_on != 0,
        sfx: config.prefs.sfx_on != 0,
    });
    if let Some(meta) = &mut active_puzzle.0 {
        meta.grid_lines = config.prefs.grid_lines_on != 0;
        meta.timer_enabled = config.prefs.timer_on != 0;
        meta.text_enabled = config.prefs.text_msg_on != 0;
    }
}

/// Replaces the active puzzle's `PuzzleAsset` (e.g., VGA <-> EGA switch) while preserving
/// all piece entity `PieceLocation`s (tray, board, held, connected groups).
pub fn replace_scene_asset_ecs(
    commands: &mut Commands,
    meta: &mut crate::ecs::resources::PuzzleMeta,
    asset: PuzzleAsset,
    piece_q: &mut Query<(
        Entity,
        &PieceIdentity,
        &mut crate::ecs::components::PieceGeometry,
        &mut PieceSprite,
    )>,
    hotspot_q: &Query<Entity, With<SceneHotspot>>,
    anim_q: &Query<Entity, With<AnimationState>>,
) {
    assert_eq!(asset.id, meta.puzzle_num);
    render_full_picture_into(&asset, &mut meta.full_picture_fb);
    let (cut_data, polylines_v, polylines_h) = cut_pieces_data(
        meta.puzzle_num,
        &asset,
        &meta.full_picture_fb,
        meta.grid_dim,
        meta.curved_edges,
    );

    let existing_count = piece_q.iter().count();
    if cut_data.len() == existing_count {
        let mut by_idx: Vec<
            Option<(
                crate::ecs::components::PieceGeometry,
                crate::assets::Sprite8061,
            )>,
        > = cut_data
            .into_iter()
            .map(|d| Some((d.geometry, d.sprite)))
            .collect();
        for (_, ident, mut geom_comp, mut sprite_comp) in piece_q.iter_mut() {
            if let Some(slot) = by_idx.get_mut(ident.index)
                && let Some((new_geom, new_spr)) = slot.take()
            {
                *geom_comp = new_geom;
                sprite_comp.sprite = new_spr;
            }
        }
    } else {
        for (e, _, _, _) in piece_q.iter() {
            commands.entity(e).despawn();
        }
        let tray_order = meta.voice_rng.shuffled_indices(cut_data.len());
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
                PieceSprite {
                    sprite: piece.sprite,
                },
                crate::ecs::components::PieceLocation::Tray { order: rank },
            ));
        }
        meta.solved = false;
        meta.toolbox_open = true;
        meta.held_group_origin_cell = None;
        meta.tray_scroll = 0;
        meta.pending_completion_feedback = false;
        meta.status_message = None;
        meta.pressed_toolbox_target = None;
    }

    for e in hotspot_q.iter() {
        commands.entity(e).despawn();
    }
    for hotspot in extract_hotspots(&asset) {
        commands.spawn(hotspot);
    }
    for e in anim_q.iter() {
        commands.entity(e).despawn();
    }

    meta.asset = asset;
    meta.grid_polylines_v = polylines_v;
    meta.grid_polylines_h = polylines_h;
    meta.drag_origin = None;
    meta.asset_rev = meta.asset_rev.wrapping_add(1);
}
