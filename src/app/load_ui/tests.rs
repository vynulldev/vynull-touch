// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

#[test]
fn playing_load_requires_confirmation_but_loaded_paused_deck_loads_directly() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            None,
        );
        let (controller, _requests, _results) = Controller::test();
        app.preview = controller;
        app.selected = Some(3);
        app.status.players[1].track_id = 2;
        app.status.players[1].on_air = true;
        app.status.players[1].play_state = 5;
        app.status.players[1].play_state_name = "PAUSED".into();
        for number in [3, 4] {
            app.status.peers.push(Peer {
                device_number: number,
                ..app.status.peers[1].clone()
            });
            app.status.players.push(Player {
                device_number: number,
                ..app.status.players[1].clone()
            });
        }
        app.status.peers.push(Peer {
            device_number: 33,
            device_type: "djm".into(),
            mixer: Some(Mixer {
                channel_state_known: true,
                channel_on_air: vec![true; 4],
            }),
            ..Default::default()
        });
        app.load_dialog = true;
        frame(&mut app, &ctx, size, vec![]);
        frame(&mut app, &ctx, size, vec![]);
        let bounds = Rect::from_min_size(Pos2::ZERO, size);
        assert!(bounds.contains_rect(app.hits.iter().find(|(n, _)| n == "Load picker").unwrap().1));
        click(&mut app, &ctx, size, "Load 1");
        assert_eq!(app.status.players[0].track_id, 1);
        assert!(app.load_confirmation.is_some());
        frame(&mut app, &ctx, size, vec![]);
        for name in ["Load confirmation", "Load confirm", "Load cancel"] {
            let rect = app.hits.iter().find(|(n, _)| n == name).unwrap().1;
            assert!(
                bounds.contains_rect(rect),
                "{name} outside screen: {rect:?}"
            );
            assert!(rect.height() >= 48.0);
        }
        click(&mut app, &ctx, size, "Load cancel");
        assert!(app.load_confirmation.is_none());
        assert!(app.load_dialog);
        assert_eq!(app.status.players[0].track_id, 1);
        click(&mut app, &ctx, size, "Load 2");
        assert_eq!(app.status.players[1].track_id, 3);
        assert!(app.load_confirmation.is_none());
        assert!(!app.load_dialog);
        app.load_dialog = true;
        click(&mut app, &ctx, size, "Load 1");
        click(&mut app, &ctx, size, "Load confirm");
        assert_eq!(app.status.players[0].track_id, 3);
        assert!(!app.status.players[0].is_playing);
        assert!(app.load_confirmation.is_none());
        assert!(!app.load_dialog);
        app.status.players[0].is_playing = true;
        app.status.players[0].play_state = 3;
        app.status.players[0].play_state_name = "PLAYING".into();
        app.inspect(app.tracks[2].clone());
        for page in [Page::Detail, Page::Players] {
            app.page = page;
            app.deck_focus = Some(1);
            app.load_on_player(app.tracks[2].clone(), app.status.peers[0].clone());
            frame(&mut app, &ctx, size, vec![]);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![eframe::egui::Event::Key {
                        key: Key::Escape,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
            }
            assert!(app.load_confirmation.is_none());
            assert_eq!(app.page, page, "Escape must only close confirmation");
            assert_eq!(app.deck_focus, Some(1));
        }
    }
}

#[test]
fn confirmation_sends_one_command_for_the_selected_track_and_captured_player() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = App::new(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    app.options.demo = false;
    app.last_status = Some(Instant::now());
    let (worker, commands, _events) = Worker::test();
    app.worker = Some(worker);
    let track = app.tracks[2].clone();
    let peer = app.status.peers[0].clone();
    app.load_on_player(track, peer.clone());
    assert!(commands.try_recv().is_err());
    app.selected = Some(4); // The confirmation keeps the track the user reviewed.
    frame(&mut app, &ctx, size, vec![]);
    click(&mut app, &ctx, size, "Load confirm");
    let Command::Load {
        track,
        peer: target,
        confirmed,
    } = commands.try_recv().unwrap()
    else {
        panic!("expected load command");
    };
    assert_eq!(track.id, 3);
    assert!(peer.same_device(&target));
    assert_eq!(confirmed.unwrap().track_id, 1);
    assert!(commands.try_recv().is_err());
    assert!(app.load_busy);
    assert!(app.load_confirmation.is_none());
}

#[test]
fn stale_status_changed_track_and_escape_cannot_confirm_a_load() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = App::new(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    app.options.demo = false;
    app.last_status = Some(Instant::now());
    let (worker, commands, _events) = Worker::test();
    app.worker = Some(worker);
    app.load_on_player(app.tracks[2].clone(), app.status.peers[0].clone());
    app.last_status = None;
    click(&mut app, &ctx, size, "Load confirm");
    assert!(commands.try_recv().is_err());
    assert!(app.load_confirmation.is_some());
    app.last_status = Some(Instant::now());
    app.status.players[0].track_id = 2;
    click(&mut app, &ctx, size, "Load confirm");
    assert!(commands.try_recv().is_err());
    assert!(app.load_confirmation.is_some());
    frame(
        &mut app,
        &ctx,
        size,
        vec![eframe::egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(app.load_confirmation.is_none());
    assert!(commands.try_recv().is_err());
}
