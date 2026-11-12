// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};
use eframe::egui::Event as InputEvent;

fn setup() -> (App, Context) {
    let ctx = Context::default();
    let mut app = App::new(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    app.preview = Controller::test().0;
    let player = app.status.players[0].clone();
    let peer = app.status.peers[0].clone();
    app.status.players = [3, 6, 5, 0x11]
        .into_iter()
        .enumerate()
        .map(|(i, state)| Player {
            device_number: i as u8 + 1,
            play_state: state,
            play_state_name: ["PLAYING", "CUED", "PAUSED", "ENDED"][i].into(),
            is_playing: i == 0,
            is_master: i == 0,
            ..player.clone()
        })
        .collect();
    app.status.peers = (1..=4)
        .map(|number| Peer {
            device_number: number,
            ip: format!("169.254.1.{number}"),
            ..peer.clone()
        })
        .collect();
    (app, ctx)
}

fn states(app: &App, track: &Track) -> Vec<(u8, State)> {
    app.on_deck_snapshot()
        .players(track)
        .map(|p| (p.number, p.state))
        .collect()
}

#[test]
fn badges_follow_each_decks_transport_track_and_source_without_title_fallback() {
    let (mut app, _) = setup();
    let track = app.all_tracks[0].clone();
    assert_eq!(
        states(&app, &track),
        [
            (1, State::Playing),
            (2, State::Cued),
            (3, State::Paused),
            (4, State::Ended)
        ]
    );
    app.status.players.truncate(1);
    for (raw, name, playing, expected) in [
        (7, "CUE PLAY", false, State::Playing),
        (4, "LOOPING", false, State::Playing),
        (3, "REVERSE", true, State::Playing),
        (5, "PAUSED", false, State::Paused),
        (6, "CUED", false, State::Cued),
        (0x11, "ENDED", false, State::Ended),
        (8, "CUE SCRATCH", false, State::Loaded),
        (9, "SEEKING", false, State::Loaded),
        (255, "UNKNOWN", false, State::Loaded),
    ] {
        let player = &mut app.status.players[0];
        player.play_state = raw;
        player.play_state_name = name.into();
        player.is_playing = playing;
        assert_eq!(states(&app, &track), [(1, expected)], "{name}");
    }
    // A new track moves the badge, even if the player title has not updated yet.
    app.status.players[0].track_id = 2;
    assert!(states(&app, &track).is_empty());
    assert_eq!(states(&app, &app.all_tracks[1]), [(1, State::Loaded)]);
    app.status.players[0].track_id = 9999;
    assert!(states(&app, &track).is_empty());
}

#[test]
fn badges_reject_external_collisions_ambiguous_devices_and_unavailable_status() {
    let (baseline, _) = setup();
    let track = baseline.all_tracks[0].clone();
    for case in 0..16 {
        let (mut app, _) = setup();
        app.status.players.truncate(1);
        match case {
            0 => app.status.players[0].external = true,
            1 => app.status.players[0].source = "USB · player 2".into(),
            2 => app.status.players[0].analysis_url = "/api/analysis/ext/2/3/1".into(),
            3 => app.status.players[0].artwork_url = "/api/artwork/ext/2/3/1".into(),
            4 => app.status.players[0].track_id = 0,
            5 => app.status.players[0].play_state = 0,
            6 => app.status.players[0].play_state = 2,
            7 => app.status.players[0].play_state_name = "EJECTING".into(),
            8 => {
                app.status.peers.remove(0);
            }
            9 => app.status.peers[0].device_type = "mixer".into(),
            10 => app.status.peers.push(app.status.peers[0].clone()),
            11 => app.status.players.push(app.status.players[0].clone()),
            12 => app.linked = false,
            13 => app.library_ready = false,
            14 => {
                app.options.demo = false;
                app.last_status = Some(Instant::now() - Duration::from_secs(5));
            }
            15 => app.all_tracks.push(track.clone()),
            _ => unreachable!(),
        };
        assert!(states(&app, &track).is_empty(), "case {case}");
    }
    let (mut app, _) = setup();
    // Missing files can still be loaded on hardware; missing library identities cannot.
    app.all_tracks[0].file_missing = true;
    assert_eq!(states(&app, &track).len(), 4);
    app.all_tracks[0].file_path = "/replacement.flac".into();
    assert!(
        states(&app, &track).is_empty(),
        "old playlist row must not match a new file"
    );
    app.all_tracks.remove(0);
    assert!(states(&app, &track).is_empty());
}

fn hit(app: &App, label: &str) -> Rect {
    app.hits
        .iter()
        .find(|(name, _)| name == label)
        .unwrap_or_else(|| panic!("Missing {label}: {:?}", app.hits))
        .1
}

fn touch(app: &mut App, ctx: &Context, size: Vec2, pos: Pos2, phase: TouchPhase) {
    let mut events = vec![
        InputEvent::PointerMoved(pos),
        InputEvent::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        },
    ];
    if phase != TouchPhase::Move {
        events.push(InputEvent::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: phase == TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    if phase == TouchPhase::End {
        events.push(InputEvent::PointerGone);
    }
    frame(app, ctx, size, events);
}

#[test]
fn four_badges_fit_collection_playlists_search_and_prepare_without_owning_taps_or_drags() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        for view in 0..4 {
            let (mut app, ctx) = setup();
            // On-deck badges are independent of the key/BPM matching preference.
            app.library_match = Reference::Off;
            if view == 1 {
                app.source(Some(app.playlists[0].clone()));
            }
            if view == 2 {
                app.keyboard = Some(Field::Search);
            }
            if view == 3 {
                app.page = Page::Prepare;
                click(&mut app, &ctx, size, "Prepare choose");
                click(&mut app, &ctx, size, "Prepare playlist 1");
            }
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![]);
            }
            let row = hit(
                &app,
                if view == 3 {
                    "Prepare track 0"
                } else {
                    "Track 1"
                },
            );
            let mut right = row.left();
            for (number, state) in [(1, "Playing"), (2, "Cued"), (3, "Paused"), (4, "Ended")] {
                let badge = hit(&app, &format!("On deck 1 P{number} {state}"));
                assert!(row.contains_rect(badge));
                assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(badge));
                assert!(badge.left() >= right);
                right = badge.right();
            }
            // All four must fit within the title cell, before the Artist column.
            if view != 3 && view != 2 {
                assert!(right < hit(&app, "Column Artist").left());
            }
            let list = if view == 3 {
                app.prepare_scroll_id()
            } else {
                app.library_scroll.unwrap()
            };
            let order = app.visible.clone();
            let revision = app.preview.request.revision;
            let pos = hit(&app, "On deck 1 P2 Cued").center();
            touch(&mut app, &ctx, size, pos, TouchPhase::Start);
            touch(
                &mut app,
                &ctx,
                size,
                pos + vec2(1.0, -1.0),
                TouchPhase::Move,
            );
            touch(&mut app, &ctx, size, pos, TouchPhase::End);
            assert_eq!(app.selected_track().map(|t| t.id), Some(1));
            assert_eq!(track_scroll::offset(&ctx, list), 0.0);
            assert_eq!(app.visible, order);
            assert_eq!(app.preview.request.revision, revision);
            assert!(app.preview.request.track.is_none());
            assert!(!app.load_dialog);
            // A drag that starts on the badge must still scroll the list.
            let pos = hit(&app, "On deck 1 P1 Playing").center();
            touch(&mut app, &ctx, size, pos, TouchPhase::Start);
            for step in 1..=3 {
                touch(
                    &mut app,
                    &ctx,
                    size,
                    pos - vec2(0.0, step as f32 * 12.0),
                    TouchPhase::Move,
                );
            }
            touch(&mut app, &ctx, size, pos - vec2(0.0, 36.0), TouchPhase::End);
            assert!(
                track_scroll::offset(&ctx, list) > 0.0,
                "view {view}, size {size:?}"
            );
            assert_eq!(app.selected_track().map(|t| t.id), Some(1));
            assert_eq!(app.preview.request.revision, revision);
        }
    }
}

#[test]
fn badges_update_and_disappear_in_place_without_changing_browsing_or_preview() {
    let (mut app, ctx) = setup();
    let size = vec2(800.0, 480.0);
    app.status.players.truncate(1);
    app.selected = Some(2);
    let order = app.visible.clone();
    frame(&mut app, &ctx, size, vec![]);
    hit(&app, "On deck 1 P1 Playing");
    app.status.players[0].track_id = 2;
    app.status.players[0].is_playing = false;
    app.status.players[0].play_state = 6;
    app.status.players[0].play_state_name = "CUED".into();
    frame(&mut app, &ctx, size, vec![]);
    hit(&app, "On deck 2 P1 Cued");
    assert!(
        !app.hits
            .iter()
            .any(|(name, _)| name.starts_with("On deck 1 "))
    );
    app.status.peers.clear();
    frame(&mut app, &ctx, size, vec![]);
    assert!(
        !app.hits
            .iter()
            .any(|(name, _)| name.starts_with("On deck "))
    );
    assert_eq!(app.selected, Some(2));
    assert_eq!(app.visible, order);
    assert!(app.preview.request.track.is_none());
}
