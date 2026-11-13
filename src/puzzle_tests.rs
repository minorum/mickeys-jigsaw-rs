use super::*;
use crate::graphics::render_scene_view;

#[test]
fn tray_bottom_padding_selects_the_last_visible_piece() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    for dim in 1..=8 {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let mut session = PuzzleSession::new(1, asset, dim, true, false, false);
        let slots = session.visible_tray_slots();
        for scroll in [0, 1, session.tray_order.len() - slots] {
            for y in [184, 185] {
                session.tray_scroll = scroll;
                let original = session.tray_order.clone();
                let last_visible = *original[scroll..scroll + slots].last().unwrap();
                session.handle_pointer_press(20, y, &mut audio);
                session.handle_pointer_release(20, y, &mut audio);
                assert_eq!(
                    session.held_piece,
                    Some(last_visible),
                    "dim={dim} scroll={scroll} y={y}"
                );
                session.handle_pointer_press(20, y, &mut audio);
                session.handle_pointer_release(20, y, &mut audio);
                assert!(session.held_piece.is_none());
                assert_eq!(session.tray_order, original);
                session.keyboard_pointer = Some((20.0, y as f32));
                assert!(session.handle_keyboard_activate(&mut audio).is_none());
                assert_eq!(session.held_piece, Some(last_visible));
                assert!(session.handle_keyboard_activate(&mut audio).is_none());
                assert!(session.held_piece.is_none());
                assert_eq!(session.tray_order, original);
            }
        }
        session.tray_scroll = 0;
        let original = session.tray_order.clone();
        session.handle_pointer_press(20, 186, &mut audio);
        session.handle_pointer_release(20, 186, &mut audio);
        assert!(session.held_piece.is_none());
        assert_eq!(session.tray_scroll, 1);
        assert_eq!(session.tray_order, original);
        session.tray_scroll = 0;
        session.tray_order.truncate(slots - 1);
        for piece in &mut session.pieces {
            piece.placed = !session.tray_order.contains(&piece.index);
        }
        let original = session.tray_order.clone();
        session.handle_pointer_press(20, 185, &mut audio);
        session.handle_pointer_release(20, 185, &mut audio);
        assert!(session.held_piece.is_none());
        assert_eq!(session.tray_order, original);
    }
}

#[test]
fn projector_dissolve_starts_from_the_presented_picture() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    let mut audio = AudioEngine::new(dir, false, false);
    for ega in [false, true] {
        let asset = PuzzleAsset::load(dir, 5, ega).unwrap();
        let toolbox =
            ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega())).unwrap();
        let mut session = PuzzleSession::new(5, asset, 2, false, false, false);
        session.solved = true;
        let playback = session
            .asset
            .hotspots
            .iter()
            .find(|h| h.playback.seq_idx == 8)
            .unwrap()
            .playback;
        let mut expected = Framebuffer::new();
        render_scene_view(
            &mut expected,
            &session.asset,
            session.asset.sequences[1].start_frame,
        );
        session.start_sequence(playback, &mut audio);
        let mut actual = Framebuffer::new();
        session.render(&mut actual, &toolbox, &font, 0, 0);
        for y in 2..198 {
            for x in 25..295 {
                assert_eq!(
                    actual.get_pixel(x, y),
                    expected.get_pixel(x, y),
                    "ega={ega} {x},{y}"
                );
            }
        }
        let anim = session.active_anim.as_ref().unwrap();
        assert_eq!(anim.transition_mode, 8);
        let frame = anim.cur_frame;
        let frame_delay = anim.frame_delay_secs;
        let duration = frame_delay.min(4.0 * DOS_TICK_SECS);
        let mut next = Framebuffer::new();
        render_scene_view(&mut next, &session.asset, frame);
        session.update(duration / 2.0, &mut audio);
        session.render(&mut actual, &toolbox, &font, 0, 0);
        let mut revealed = vec![false; 64000];
        for &pixel in crate::graphics::dissolve_order(ega).iter().take(32000) {
            revealed[pixel as usize] = true;
        }
        let (mut old_pixels, mut new_pixels) = (0, 0);
        for y in 2..198 {
            for x in 25..295 {
                let pixel = (y * 320 + x - SCENE_OFFSET_X) as usize;
                let before = expected.get_pixel(x, y);
                let after = next.get_pixel(x, y);
                if before != after {
                    if revealed[pixel] {
                        new_pixels += 1
                    } else {
                        old_pixels += 1
                    }
                }
                let color = if revealed[pixel] { after } else { before };
                assert_eq!(actual.get_pixel(x, y), color, "ega={ega} midpoint {x},{y}");
            }
        }
        assert!(old_pixels > 0 && new_pixels > 0);
        session.update(duration / 2.0, &mut audio);
        session.render(&mut actual, &toolbox, &font, 0, 0);
        for y in 2..198 {
            for x in 25..295 {
                assert_eq!(actual.get_pixel(x, y), next.get_pixel(x, y));
            }
        }
        session.update(frame_delay - duration, &mut audio);
        let anim = session.active_anim.as_ref().unwrap();
        assert_eq!(anim.previous_frame, frame);
        assert_eq!(anim.cur_frame, frame + 1);
        assert_eq!(anim.transition_mode, 8);
        let delay = anim.frame_delay_secs;
        session.update(delay * 0.75, &mut audio);
        session.render(&mut actual, &toolbox, &font, 0, 0);
        render_scene_view(&mut next, &session.asset, frame + 1);
        for y in 2..198 {
            for x in 25..295 {
                assert_eq!(
                    actual.get_pixel(x, y),
                    next.get_pixel(x, y),
                    "ega={ega} final frame {x},{y}"
                );
            }
        }
        session.handle_click(100, 100, &mut audio);
        assert!(session.active_anim.is_none());
    }
}

#[test]
fn robot_hotspot_repeats_its_native_sequence() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 2, false).unwrap();
    let mut session = PuzzleSession::new(2, asset, 2, false, false, false);
    let mut audio = AudioEngine::new(dir, false, false);
    let playback = session
        .asset
        .hotspots
        .iter()
        .find(|h| h.playback.seq_idx == 4)
        .unwrap()
        .playback;
    session.start_sequence(playback, &mut audio);
    let start = session.active_anim.as_ref().unwrap().cur_frame;
    let length = session.active_anim.as_ref().unwrap().end_frame - start;
    for _ in 0..length {
        let delay = session.active_anim.as_ref().unwrap().frame_delay_secs;
        session.update(delay, &mut audio);
    }
    assert_eq!(
        session.active_anim.as_ref().map(|a| a.cur_frame),
        Some(start)
    );
}

#[test]
fn sequence_repetitions_and_delays_match_original_machine_code() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    let mut checked = 0;
    for row in include_str!("../validation/sequence-playback.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        let repetitions: u16 = fields[0].parse().unwrap();
        let ticks: u16 = fields[1].parse().unwrap();
        let explicit: u16 = fields[2].parse().unwrap();
        let frames: Vec<usize> = fields[3].split(',').map(|v| v.parse().unwrap()).collect();
        let delays: Vec<u16> = fields[4].split(',').map(|v| v.parse().unwrap()).collect();
        let mut asset = PuzzleAsset::load(dir, 1, false).unwrap();
        asset.records = vec![SceneRecord::SequenceHeader {
            name: "FILM".into(),
            default_playback: Some(SequencePlayback {
                seq_idx: 2,
                repetitions,
                frame_delay_ticks: ticks,
            }),
            hotspots: Vec::new(),
        }];
        for frame in 1..=3 {
            asset.records.push(SceneRecord::Frame {
                bg_frame_idx: 0,
                bg_color: 0,
                commands: if frame == 2 && explicit != 0 {
                    vec![FrameCommand::Delay {
                        raw: explicit << 6,
                        ticks: explicit,
                    }]
                } else {
                    Vec::new()
                },
            });
        }
        asset.sequences[1].header_idx = 0;
        asset.sequences[2].start_frame = 1;
        asset.sequences[2].end_frame = 4;
        let mut session = PuzzleSession::new(1, asset, 2, false, false, false);
        session.on_puzzle_completed(&mut audio);
        assert_eq!(frames.len(), delays.len());
        for (frame, delay) in frames.into_iter().zip(delays) {
            let anim = session.active_anim.as_ref().unwrap();
            assert_eq!(anim.cur_frame, frame, "{row}");
            assert_eq!(
                anim.frame_delay_secs,
                f32::from(delay) * DOS_TICK_SECS,
                "{row}"
            );
            session.update(anim.frame_delay_secs, &mut audio);
        }
        assert!(session.active_anim.is_none(), "{row}");
        checked += 1;
    }
    assert_eq!(checked, 48);
}

#[test]
fn original_preset_pieces_can_be_placed_in_both_video_modes() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    for ega in [false, true] {
        for num in 1..=15 {
            let asset = PuzzleAsset::load(dir, num, ega).unwrap();
            let mut session = PuzzleSession::new(num, asset, 1, true, true, false);
            assert_eq!(session.grid_dim, 1);
            assert!((11..=16).contains(&session.pieces.len()));
            for ordinal in 0..session.pieces.len() {
                session.handle_pointer_press(20, 40, &mut audio);
                let held = session.held_piece.unwrap();
                let piece = &session.pieces[held];
                assert!(piece.sprite.opacity.as_ref().unwrap().iter().any(|&v| v));
                let (x, y) = (
                    session.playfield_x() + piece.base_x + piece.base_w / 2,
                    session.playfield_y() + piece.base_y + piece.base_h / 2,
                );
                if ordinal % 2 == 0 {
                    session.handle_pointer_release(x, y, &mut audio);
                } else {
                    session.handle_pointer_release(20, 40, &mut audio);
                    session.handle_pointer_press(x, y, &mut audio);
                    session.handle_pointer_release(x, y, &mut audio);
                }
                assert!(
                    session.pieces[held].placed,
                    "P{num:02} ega={ega} piece={held}"
                );
            }
            assert!(session.solved);
        }
    }
}

#[test]
fn preset_board_keeps_native_layers_and_viewport_clipping() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        for num in 1..=15 {
            let asset = PuzzleAsset::load(dir, num, ega).unwrap();
            let toolbox =
                ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega()))
                    .unwrap();
            let mut session = PuzzleSession::new(num, asset, 1, true, true, false);
            for phase in 0..8 {
                session.toolbox_open = phase < 4;
                let phase = phase % 4;
                for piece in &mut session.pieces {
                    piece.placed = phase == 3 || (phase != 0 && piece.index % 2 == phase - 1);
                    assert!(piece.base_w > 0 && piece.base_h > 0);
                    assert!(piece.base_x >= 0 && piece.base_y >= 0);
                    assert!(piece.base_x + piece.base_w <= PLAYFIELD_W);
                    assert!(piece.base_y + piece.base_h <= PLAYFIELD_H);
                }
                let mut expected_asset = session.asset.clone();
                let frame_idx = session.asset.preset_frame_idx();
                if let SceneRecord::Frame { commands, .. } =
                    &mut expected_asset.records[frame_idx]
                {
                    for (piece, command) in session.pieces.iter().zip(commands) {
                        if let FrameCommand::SubFrame { flags, .. } = command {
                            *flags = if piece.placed {
                                *flags & 3
                            } else {
                                (*flags & 0xff0f) | 0x20
                            };
                        }
                    }
                }
                let mut expected = Framebuffer::new();
                render_scene_frame(
                    &mut expected,
                    &expected_asset,
                    frame_idx,
                    0,
                    0,
                    0,
                    true,
                    0,
                    0,
                );
                let mut actual = Framebuffer::new();
                session.render(&mut actual, &toolbox, &font, 0, 0);
                for y in 2..198 {
                    for x in 2..272 {
                        assert_eq!(
                            actual.get_pixel(session.playfield_x() + x - 2, y),
                            expected.get_pixel(x, y),
                            "P{num:02} ega={ega} phase={phase} {x},{y}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn completed_picture_matches_the_assembled_board_pixels() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    for ega in [false, true] {
        for id in 1..=15 {
            let asset = PuzzleAsset::load(dir, id, ega).unwrap();
            let toolbox =
                ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega()))
                    .unwrap();
            let mut session = PuzzleSession::new(id, asset, 3, true, false, false);
            session.toolbox_open = false;
            for piece in &mut session.pieces {
                piece.placed = true;
            }
            let mut board = Framebuffer::new();
            session.render(&mut board, &toolbox, &font, 0, 0);
            session.solved = true;
            let mut picture = Framebuffer::new();
            session.render(&mut picture, &toolbox, &font, 0, 0);
            for y in 2..198 {
                for x in 25..295 {
                    assert_eq!(
                        board.get_pixel(x, y),
                        picture.get_pixel(x, y),
                        "P{id:02} ega={ega} {x},{y}"
                    );
                }
            }
        }
    }
}

#[test]
fn grid_pieces_match_original_capture_rectangles_and_source_pixels() {
    let asset = PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 1, false).unwrap();
    let sessions: Vec<_> = (2..=8)
        .map(|dim| PuzzleSession::new(1, asset.clone(), dim, false, false, false))
        .collect();
    let mut checked = 0;
    for row in include_str!("../validation/piece-capture.tsv")
        .lines()
        .skip(1)
    {
        let v: Vec<i32> = row.split('\t').map(|word| word.parse().unwrap()).collect();
        let session = &sessions[v[0] as usize - 2];
        let piece = &session.pieces[v[1] as usize];
        assert_eq!(
            (
                2 + piece.base_x,
                2 + piece.base_y,
                piece.base_w,
                piece.base_h
            ),
            (v[2], v[3], v[4], v[5]),
            "{row}"
        );
        for y in 0..piece.sprite.sub_h as i32 {
            for x in 0..piece.sprite.width as i32 {
                let index = (y * piece.sprite.stride as i32 + x) as usize;
                if piece.sprite.is_opaque(index) {
                    let source_x = v[2] + piece.target_x - piece.base_x + x;
                    let source_y = v[3] + piece.target_y - piece.base_y + y;
                    assert_eq!(
                        piece.sprite.pixels[index],
                        session.full_picture_fb.get_pixel(source_x, source_y),
                        "{row}: {x},{y}"
                    );
                }
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 203);
}

#[test]
fn keyboard_pointer_can_complete_every_grid_and_yield_to_mouse() {
    fn move_to(session: &mut PuzzleSession, x: i32, y: i32) {
        let (old_x, old_y) = session.keyboard_pointer.unwrap_or((20.0, 40.0));
        let dx = x as f32 - old_x;
        let dy = y as f32 - old_y;
        session.move_keyboard_pointer(dx.signum(), 0.0, dx.abs() / 120.0);
        session.move_keyboard_pointer(0.0, dy.signum(), dy.abs() / 120.0);
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    for dim in 1..=8 {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let mut session = PuzzleSession::new(1, asset, dim, true, false, false);
        for _ in 0..session.pieces.len() {
            move_to(&mut session, 20, 40);
            assert!(session.handle_keyboard_activate(&mut audio).is_none());
            let held = session.held_piece.unwrap();
            let piece = &session.pieces[held];
            let (x, y) = (
                session.playfield_x() + piece.base_x + piece.base_w / 2,
                session.playfield_y() + piece.base_y + piece.base_h / 2,
            );
            move_to(&mut session, x, y);
            session.handle_keyboard_activate(&mut audio);
            assert!(session.pieces[held].placed);
        }
        assert!(session.solved);
        session.handle_keyboard_activate(&mut audio);
        assert!(session.active_anim.is_none());
        move_to(&mut session, 2, 2);
        assert_eq!(
            session.handle_keyboard_activate(&mut audio),
            Some(PuzzleAction::OpenMenu)
        );
        session.move_keyboard_pointer(-1.0, -1.0, 100.0);
        assert_eq!(session.keyboard_pointer, Some((0.0, 0.0)));
        session.move_keyboard_pointer(1.0, 1.0, 100.0);
        assert_eq!(session.keyboard_pointer, Some((319.0, 199.0)));
        session.handle_pointer_press(2, 2, &mut audio);
        assert!(session.keyboard_pointer.is_none());
    }
}

#[test]
fn speech_captions_survive_muting_both_audio_options() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    let mut checked = 0;
    for ega in [false, true] {
        for id in 1..=15 {
            let asset = PuzzleAsset::load(dir, id, ega).unwrap();
            let candidates: Vec<_> = asset
                .sounds
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    let music = entry.sounds.first()?;
                    (audio
                        .iss_songs
                        .contains_key(&(music.stem.clone(), music.phrase_idx as usize))
                        && !entry.bubbles.is_empty()
                        && entry.bubbles.iter().all(Option::is_some))
                    .then_some(index)
                })
                .collect();
            let mut session = PuzzleSession::new(id, asset, 2, false, false, false);
            session.text_enabled = true;
            for sound in candidates {
                let frame = session.asset.records.iter().position(|record| matches!(record,
                    SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                        matches!(command, FrameCommand::Sound { snd_idx: Some(index) } if *index == sound)))).unwrap();
                session.active_anim = Some(AnimationPlayer {
                    seq_idx: 0,
                    start_frame: frame,
                    previous_frame: frame,
                    cur_frame: frame,
                    end_frame: frame + 1,
                    tick_accum: 0.0,
                    frame_delay_secs: 0.0,
                    repetitions_left: 1,
                    default_delay_secs: 2.0 * DOS_TICK_SECS,
                    transition_mode: 0,
                    bubble: None,
                    wait_for_audio: false,
                    looping_sound: None,
                });
                session.trigger_frame_side_effects(frame, &mut audio);
                assert!(
                    session.active_anim.as_ref().unwrap().bubble.is_some(),
                    "P{id:02} ega={ega} sound={}",
                    session.asset.sounds[sound].tag
                );
                session.text_enabled = false;
                session.trigger_frame_side_effects(frame, &mut audio);
                assert!(session.active_anim.as_ref().unwrap().bubble.is_none());
                session.text_enabled = true;
                audio.music_enabled = true;
                session.trigger_frame_side_effects(frame, &mut audio);
                assert!(session.active_anim.as_ref().unwrap().bubble.is_none());
                audio.music_enabled = false;
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 2);
    println!("{checked} music/speech entries retain captions with both channels muted");
}

#[test]
fn tray_toggle_cancels_drag_without_losing_held_piece() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    for dim in 2..=8 {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let mut session = PuzzleSession::new(1, asset, dim, true, true, true);
        session.handle_pointer_press(20, 40, &mut audio);
        let held = session.held_piece.unwrap();
        session.toggle_toolbox();
        let piece = &session.pieces[held];
        let target = (
            session.playfield_x() + piece.base_x + piece.base_w / 2,
            session.playfield_y() + piece.base_y + piece.base_h / 2,
        );
        session.handle_pointer_release(target.0, target.1, &mut audio);
        assert_eq!(session.held_piece, Some(held));
        assert!(!session.pieces[held].placed);
        assert!(!session.tray_order.contains(&held));
        session.handle_pointer_press(target.0, target.1, &mut audio);
        assert!(session.pieces[held].placed);
        session.handle_pointer_release(target.0, target.1, &mut audio);
        session.toggle_toolbox();
        assert!(session.toolbox_open);
    }
}

#[test]
fn drag_release_places_pieces_without_breaking_click_to_carry() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    for dim in 2..=8 {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let mut session = PuzzleSession::new(1, asset, dim, true, true, true);
        session.handle_pointer_press(20, 40, &mut audio);
        let held = session.held_piece.unwrap();
        session.handle_pointer_release(21, 41, &mut audio);
        assert_eq!(session.held_piece, Some(held));
        let piece = &session.pieces[held];
        let target = (
            session.playfield_x() + piece.base_x + piece.base_w / 2,
            session.playfield_y() + piece.base_y + piece.base_h / 2,
        );
        session.handle_pointer_press(target.0, target.1, &mut audio);
        session.handle_pointer_release(target.0, target.1, &mut audio);
        assert!(session.pieces[held].placed);
        for _ in 1..dim * dim {
            session.handle_pointer_press(20, 40, &mut audio);
            let held = session.held_piece.unwrap();
            let piece = &session.pieces[held];
            let target = (
                session.playfield_x() + piece.base_x + piece.base_w / 2,
                session.playfield_y() + piece.base_y + piece.base_h / 2,
            );
            session.handle_pointer_release(target.0, target.1, &mut audio);
            assert!(session.pieces[held].placed);
        }
        assert!(session.solved);
        assert!(session.tray_order.is_empty());
    }
}

#[test]
fn drag_release_outside_or_over_controls_keeps_the_piece() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    for release in [
        (-1, 40),
        (320, 40),
        (20, -1),
        (20, 200),
        (10, 10),
        (10, 20),
        (10, 190),
    ] {
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let mut session = PuzzleSession::new(1, asset, 3, true, true, true);
        session.handle_pointer_press(20, 40, &mut audio);
        let held = session.held_piece.unwrap();
        session.handle_pointer_release(release.0, release.1, &mut audio);
        assert_eq!(session.held_piece, Some(held), "{release:?}");
        assert!(!session.pieces[held].placed);
        assert!(!session.tray_order.contains(&held));
        assert_eq!(session.tray_scroll, 0);
    }
}

#[test]
fn cancelled_drag_keeps_piece_and_tray_drop_returns_it_once() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut audio = AudioEngine::new(dir, false, false);
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut session = PuzzleSession::new(1, asset, 3, true, true, true);
    session.handle_pointer_press(20, 40, &mut audio);
    let held = session.held_piece.unwrap();
    let piece = &session.pieces[held];
    let target = (
        session.playfield_x() + piece.base_x + piece.base_w / 2,
        session.playfield_y() + piece.base_y + piece.base_h / 2,
    );
    session.cancel_pointer_drag();
    session.handle_pointer_release(target.0, target.1, &mut audio);
    assert_eq!(session.held_piece, Some(held));
    assert!(!session.pieces[held].placed);
    session.handle_pointer_press(20, 40, &mut audio);
    assert!(session.held_piece.is_none());
    session.handle_pointer_release(20, 40, &mut audio);
    session.handle_pointer_press(20, 40, &mut audio);
    let held = session.held_piece.unwrap();
    session.handle_pointer_release(20, 130, &mut audio);
    assert!(session.held_piece.is_none());
    assert_eq!(
        session
            .tray_order
            .iter()
            .filter(|&&index| index == held)
            .count(),
        1
    );
    assert_eq!(session.tray_order.len(), 9);
}

#[test]
fn supplied_repeated_effects_survive_frames_and_cancel() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (id, ega, tag) in [
        (2, false, "ROBOT"),
        (2, true, "ROBOT"),
        (6, false, "SKATE"),
        (13, false, "HORSWALK"),
        (13, true, "HORSWALK"),
    ] {
        let mut asset = PuzzleAsset::load(dir, id, ega).unwrap();
        let sound = asset
            .sounds
            .iter()
            .position(|entry| entry.tag == tag)
            .unwrap();
        let frame = asset.records.iter().position(|record| matches!(record,
            SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                matches!(command, FrameCommand::Sound { snd_idx: Some(index) } if *index == sound))
        )).unwrap();
        asset.sequences[2].start_frame = frame;
        asset.sequences[2].end_frame = frame + 2;
        let mut session = PuzzleSession::new(id, asset, 2, false, false, false);
        let mut audio = AudioEngine::new(dir, false, false);
        audio.sfx_enabled = true;
        session.on_puzzle_completed(&mut audio);
        assert_eq!(
            session.active_anim.as_ref().unwrap().looping_sound,
            Some((sound, 1)),
            "{id} {ega}"
        );
        let delay = session.active_anim.as_ref().unwrap().frame_delay_secs;
        session.update(delay, &mut audio);
        assert_eq!(
            session.active_anim.as_ref().unwrap().looping_sound,
            Some((sound, 1)),
            "{id} {ega}"
        );
        let replacement = session
            .asset
            .records
            .iter()
            .position(|record| {
                matches!(record,
                    SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                        matches!(command, FrameCommand::Sound { snd_idx: Some(index) }
                            if session.asset.sounds[*index].selection_mode & 0x80 == 0))
                )
            })
            .unwrap();
        session.trigger_frame_side_effects(replacement, &mut audio);
        assert!(
            session
                .active_anim
                .as_ref()
                .unwrap()
                .looping_sound
                .is_none()
        );
        session.on_puzzle_completed(&mut audio);
        session.handle_click(160, 100, &mut audio);
        assert!(session.active_anim.is_none());
        assert!(!audio.is_playing());
    }
}

#[test]
fn original_dialogue_is_visible_and_waits_when_muted() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for ega in [false, true] {
        let asset = PuzzleAsset::load(dir, 1, ega).unwrap();
        let sound = asset
            .sounds
            .iter()
            .position(|entry| entry.tag == "OHBOY-01")
            .unwrap();
        let frame = asset.records.iter().position(|record| matches!(record,
        SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
            matches!(command, FrameCommand::Sound { snd_idx: Some(index) } if *index == sound))
    )).unwrap();
        let mut session = PuzzleSession::new(1, asset, 2, false, false, false);
        let mut audio = AudioEngine::new(dir, false, false);
        session.active_anim = Some(AnimationPlayer {
            seq_idx: 2,
            start_frame: frame,
            previous_frame: frame,
            cur_frame: frame,
            end_frame: frame + 2,
            tick_accum: 0.0,
            frame_delay_secs: 0.0,
            repetitions_left: 1,
            default_delay_secs: 2.0 * DOS_TICK_SECS,
            transition_mode: 0,
            bubble: None,
            wait_for_audio: false,
            looping_sound: None,
        });
        session.trigger_frame_side_effects(frame, &mut audio);
        let anim = session.active_anim.as_ref().unwrap();
        assert_eq!(anim.bubble, Some((sound, 0)));
        assert!(anim.wait_for_audio);
        assert_eq!(anim.frame_delay_secs, 20.0 * DOS_TICK_SECS);
        let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
        let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).unwrap();
        let mut fb = Framebuffer::new();
        session.render(&mut fb, &toolbox, &font, 0, 0);
        assert_eq!(
            fb.pixels[80 * 320 + 10 + SCENE_OFFSET_X as usize],
            if ega { 15 } else { 248 }
        );
        assert_eq!(
            fb.pixels[125 * 320 + 33 + SCENE_OFFSET_X as usize],
            if ega { 1 } else { 16 }
        );
        session.update(1.0, &mut audio);
        assert_eq!(session.active_anim.as_ref().unwrap().cur_frame, frame);
        session.update(0.2, &mut audio);
        assert_eq!(session.active_anim.as_ref().unwrap().cur_frame, frame + 1);
        assert_eq!(session.active_anim.as_ref().unwrap().bubble, None);
        session.text_enabled = false;
        session.trigger_frame_side_effects(frame, &mut audio);
        assert_eq!(session.active_anim.as_ref().unwrap().bubble, None);
        assert_eq!(
            session.active_anim.as_ref().unwrap().frame_delay_secs,
            2.0 * DOS_TICK_SECS
        );
    }
}

#[test]
fn solved_clicks_select_complete_character_groups() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut session = PuzzleSession::new(1, asset, 2, false, false, false);
    session.solved = true;
    let mut audio = AudioEngine::new(dir, false, false);
    for (x, y, expected) in [
        (188, 24, "F_GOOFY"),
        (192, 105, "F_GOOFY"),
        (202, 168, "F_GOOFY"),
        (131, 178, "F_MICKEY"),
        (155, 24, "F_ALL"),
        (319, 199, "F_ALL"),
    ] {
        session.active_anim = None;
        let display_x = if x < 273 { x + SCENE_OFFSET_X } else { x };
        assert!(session.handle_click(display_x, y, &mut audio).is_none());
        let selected = session.active_anim.as_ref().unwrap().seq_idx;
        assert_eq!(session.asset.sequences[selected].name, expected, "{x},{y}");
    }
}

#[test]
fn solved_hotspot_centers_keep_original_selection() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let session = PuzzleSession::new(1, asset, 2, false, false, false);
    for hotspot in &session.asset.hotspots {
        assert_eq!(
            session.solved_sequence_at(
                hotspot.x as i32 + hotspot.width as i32 / 2,
                hotspot.y as i32 + hotspot.height as i32 / 2,
            ),
            hotspot.playback,
        );
    }
}

#[test]
fn all_solved_hotspot_centers_select_and_complete_their_animation() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    let mut audio = AudioEngine::new(dir, false, false);
    let mut checked = 0;
    for ega in [false, true] {
        for id in 1..=15 {
            let asset = PuzzleAsset::load(dir, id, ega).unwrap();
            let toolbox =
                ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega()))
                    .unwrap();
            let mut session = PuzzleSession::new(id, asset, 2, false, false, false);
            session.solved = true;
            let header = session.asset.sequences[1].header_idx;
            let SceneRecord::SequenceHeader { hotspots, .. } = &session.asset.records[header]
            else {
                panic!("Missing FILM header");
            };
            for hotspot in hotspots.clone() {
                let x = hotspot.x as i32 + hotspot.width as i32 / 2;
                let y = hotspot.y as i32 + hotspot.height as i32 / 2;
                assert_eq!(
                    session.solved_sequence_at(x, y),
                    hotspot.playback,
                    "P{id:02} ega={ega} center={x},{y}"
                );
                assert!(
                    session
                        .handle_click(x + SCENE_OFFSET_X, y, &mut audio)
                        .is_none()
                );
                assert_eq!(
                    session.active_anim.as_ref().unwrap().seq_idx,
                    hotspot.playback.seq_idx
                );
                let anim = session.active_anim.as_ref().unwrap();
                assert_eq!(anim.repetitions_left, hotspot.playback.repetitions.max(1));
                assert_eq!(
                    anim.default_delay_secs,
                    f32::from(hotspot.playback.frame_delay_ticks) * DOS_TICK_SECS
                );
                let mut fb = Framebuffer::new();
                session.render(&mut fb, &toolbox, &font, x, y);
                for _ in 0..2000 {
                    if session.active_anim.is_none() {
                        break;
                    }
                    session.update(1.0, &mut audio);
                }
                assert!(
                    session.active_anim.is_none(),
                    "P{id:02} ega={ega} sequence={}",
                    hotspot.playback.seq_idx
                );
                session.render(&mut fb, &toolbox, &font, x, y);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 134);
    println!("{checked} hotspot animations completed across 30 scene selections");
}

#[test]
fn tray_scroll_clamps_to_visible_bounds() {
    let asset = PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 1, false).unwrap();
    let mut session = PuzzleSession::new(1, asset, 3, true, false, false);
    // 3x3 has 9 pieces and 3 visible tray slots -> max scroll = 6.
    assert_eq!(session.visible_tray_slots(), 3);
    session.scroll_tray(-5);
    assert_eq!(session.tray_scroll, 0);
    session.scroll_tray(20);
    assert_eq!(session.tray_scroll, 6);
    session.scroll_tray(-2);
    assert_eq!(session.tray_scroll, 4);

    // When remaining pieces fit in visible slots, scroll stays 0.
    session.tray_order.truncate(2);
    session.tray_scroll = 0;
    session.scroll_tray(1);
    assert_eq!(session.tray_scroll, 0);
}

#[test]
fn board_cache_updates_across_toggles_and_placements() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).unwrap();
    let mut session = PuzzleSession::new(1, asset, 3, true, false, false);

    let mut fb_initial = Framebuffer::new();
    session.render(&mut fb_initial, &toolbox, &font, 0, 0);

    session.grid_lines = true;
    let mut fb_grid = Framebuffer::new();
    session.render(&mut fb_grid, &toolbox, &font, 0, 0);
    assert_ne!(fb_initial.pixels, fb_grid.pixels);

    session.toolbox_open = false;
    let mut fb_no_tbx = Framebuffer::new();
    session.render(&mut fb_no_tbx, &toolbox, &font, 0, 0);
    assert_ne!(fb_grid.pixels, fb_no_tbx.pixels);

    session.pieces[0].placed = true;
    let mut fb_placed = Framebuffer::new();
    session.render(&mut fb_placed, &toolbox, &font, 0, 0);
    assert_ne!(fb_no_tbx.pixels, fb_placed.pixels);
}

#[test]
fn render_reference_matches_scene_view() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for ega in [false, true] {
        let asset = PuzzleAsset::load(dir, 1, ega).unwrap();
        let session = PuzzleSession::new(1, asset, 3, true, false, false);
        let mut expected = Framebuffer::new();
        render_scene_view(
            &mut expected,
            &session.asset,
            session.asset.static_frame_idx(),
        );
        let mut actual = Framebuffer::new();
        session.render_reference(&mut actual);
        assert_eq!(actual.pixels, expected.pixels, "ega={ega}");
        assert_eq!(actual.palette, expected.palette, "ega={ega}");
    }
}

#[test]
fn update_ignores_non_finite_and_negative_dt() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut session = PuzzleSession::new(1, asset, 3, true, false, true);
    let mut audio = AudioEngine::new(dir, false, false);
    session.elapsed_secs = 12.5;
    session.hint_piece = Some((0, 1.0));
    session.status_message = Some(("OK".to_string(), 2.0));

    for bad_dt in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        session.update(bad_dt, &mut audio);
        assert_eq!(session.elapsed_secs, 12.5);
        assert_eq!(session.hint_piece, Some((0, 1.0)));
        assert_eq!(session.status_message.as_ref().unwrap().1, 2.0);
    }
}

#[test]
fn scroll_tray_handles_extreme_deltas() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut session = PuzzleSession::new(1, asset, 3, true, false, false);
    session.scroll_tray(i32::MAX);
    assert_eq!(session.tray_scroll, 6);
    session.scroll_tray(i32::MIN);
    assert_eq!(session.tray_scroll, 0);
}

#[test]
fn replace_scene_asset_preserves_solved_and_partial_state() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let vga = PuzzleAsset::load(dir, 1, false).unwrap();
    let ega = PuzzleAsset::load(dir, 1, true).unwrap();
    let mut session = PuzzleSession::new(1, vga, 3, true, true, true);
    session.pieces[0].placed = true;
    session.tray_order.retain(|&idx| idx != 0);
    let held = session.tray_order.remove(0);
    session.held_piece = Some(held);
    session.tray_scroll = 2;
    session.elapsed_secs = 42.0;

    let expected_tray = session.tray_order.clone();
    session.replace_scene_asset(ega);
    assert!(session.asset.uses_ega());
    assert!(session.pieces[0].placed);
    assert_eq!(session.held_piece, Some(held));
    assert_eq!(session.tray_order, expected_tray);
    assert_eq!(session.tray_scroll, 2);
    assert_eq!(session.elapsed_secs, 42.0);
    assert!(!session.solved);

    let vga2 = PuzzleAsset::load(dir, 1, false).unwrap();
    let mut audio = AudioEngine::new(dir, false, false);
    session.auto_solve(&mut audio);
    assert!(session.solved);
    session.replace_scene_asset(vga2);
    assert!(session.solved);
    assert!(!session.toolbox_open);
}
