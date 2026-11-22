// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

fn app(ctx: &Context) -> App {
    let mut app = App::new(
        ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    app.preview = Controller::test().0;
    app.page = Page::Players;
    app
}

#[test]
fn mixer_presence_and_partial_channel_reports_do_not_invent_off_air_state() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.status = serde_json::from_str(r#"{"peers":[
        {"device_number":2,"name":"CDJ-3000","device_type":"cdj","ip":"169.254.1.2","mac":""},
        {"device_number":33,"name":"DJM-A9","device_type":"mixer","ip":"169.254.1.33","mac":"00:11:22:33:44:55"},
        {"device_number":34,"name":"DJM-900NXS2","device_type":"mixer","ip":"","mac":"","mixer":{"channel_state_known":true,"channel_on_air":[true,false]}},
        {"device_number":35,"name":"Other mixer","device_type":"djm","ip":"","mac":"","mixer":{"channel_state_known":false,"channel_on_air":[true,false,false,false]}},
        {"device_number":36,"name":"","device_type":"mixer","ip":"","mac":"","mixer":{"channel_state_known":true,"channel_on_air":null}},
        {"device_number":17,"name":"Laptop","device_type":"rekordbox","ip":"","mac":""},
        {"device_number":18,"name":"","device_type":"future-device","ip":"","mac":""}
    ],"players":[]}"#).unwrap();
    assert_eq!(channels(&app.status.peers[1], true), [None; 4]);
    assert_eq!(
        channels(&app.status.peers[2], true),
        [Some(true), Some(false), None, None]
    );
    assert_eq!(channels(&app.status.peers[3], true), [None; 4]);
    assert_eq!(channels(&app.status.peers[4], true), [None; 4]);
    let report = app.diagnostic_report();
    assert!(report.contains("DJM-A9 · Detected\nMixer · Device 33 · IP 169.254.1.33 · MAC 00:11:22:33:44:55\nCH1: unknown"));
    assert!(report.contains("CH1: on air · CH2: off air · CH3: unknown · CH4: unknown"));
    assert!(report.contains("Laptop"));
    assert!(report.contains("future-device"));
    let peers = app.network_peers();
    assert_eq!(
        peers.iter().map(|p| p.device_number).collect::<Vec<_>>(),
        vec![33, 34, 35, 36, 2, 17, 18]
    );
}

#[test]
fn network_recovery_removal_and_link_off_keep_presence_distinct_from_live_channels() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.options.demo = false;
    let (worker, _commands, events) = Worker::test();
    app.worker = Some(worker);
    let mut status = demo::status();
    status.peers[2].mixer = Some(Mixer {
        channel_state_known: true,
        channel_on_air: vec![true, false, false, false],
    });
    events
        .send(Event::Status(Instant::now(), Ok((status.clone(), false))))
        .unwrap();
    app.poll(&ctx);
    assert!(app.network_report().contains("Link inactive"));
    assert!(app.network_report().contains("DJM-A9 · Detected"));
    assert!(app.network_report().contains("CH1: on air"));
    events
        .send(Event::Status(Instant::now(), Err("Offline".into())))
        .unwrap();
    app.poll(&ctx);
    let report = app.network_report();
    assert!(report.contains("Status unavailable · last known devices"));
    assert!(report.contains("DJM-A9 · Last seen"));
    assert!(
        !report.contains("Detected") && !report.contains("on air") && !report.contains("off air")
    );
    events
        .send(Event::Status(
            Instant::now() - Duration::from_secs(5),
            Ok((status.clone(), true)),
        ))
        .unwrap();
    app.poll(&ctx);
    assert!(
        app.network_report().contains("Last seen"),
        "old successful status must also expire"
    );
    status.peers.retain(|p| !p.is_mixer());
    events
        .send(Event::Status(Instant::now(), Ok((status, true))))
        .unwrap();
    app.poll(&ctx);
    assert!(!app.network_report().contains("DJM-A9"));
    assert!(app.network_report().contains("2 devices detected"));
    events
        .send(Event::Status(Instant::now(), Ok((Status::default(), true))))
        .unwrap();
    app.poll(&ctx);
    assert!(app.network_report().contains("0 devices detected"));
}

#[test]
fn network_shortcut_fits_all_player_views_and_opens_touch_device_list() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        for (compare, focus) in [(false, None), (true, None), (true, Some(1))] {
            let ctx = Context::default();
            let mut app = app(&ctx);
            app.options.demo = false;
            app.last_status = Some(Instant::now());
            app.deck_compare = compare;
            app.deck_focus = focus;
            frame(&mut app, &ctx, size, vec![]);
            let header = Rect::from_min_size(Pos2::ZERO, vec2(size.x, 52.0));
            let mut previous_right = 0.0;
            for label in [
                "Compare zoom −",
                "Compare zoom +",
                "Deck compare",
                "Deck pair",
                "Match a player",
                "Network devices",
                "Connection status",
                "Setup",
            ] {
                if let Some((_, rect)) = app.hits.iter().find(|(n, _)| n == label) {
                    assert!(
                        header.contains_rect(*rect),
                        "{label} outside header at {size:?}: {rect:?}"
                    );
                    assert!(
                        rect.left() >= previous_right,
                        "{label} overlaps previous toolbar control at {size:?}"
                    );
                    previous_right = rect.right();
                }
            }
            let indicator = app
                .hits
                .iter()
                .find(|(n, _)| n == "Network devices")
                .unwrap()
                .1;
            assert!(indicator.width() >= 48.0 && indicator.height() >= 48.0);
            click(&mut app, &ctx, size, "Network devices");
            assert_eq!(app.page, Page::Diag);
            assert_eq!(app.diagnostics.tab, diag_ui::Tab::Network);
            assert!(app.diagnostics.logs.updated.is_none());
            assert!(app.diagnostics.version.updated.is_none());
            frame(&mut app, &ctx, size, vec![]);
            let bounds = Rect::from_min_size(Pos2::ZERO, size);
            for label in [
                "Diag back",
                "Diag tab Status",
                "Diag tab Logs",
                "Diag tab Network",
                "Copy diagnostics",
            ] {
                let rect = app.hits.iter().find(|(n, _)| n == label).unwrap().1;
                assert!(bounds.contains_rect(rect), "{label} outside screen");
                assert!(rect.width() >= 48.0 && rect.height() >= 48.0);
            }
            click(&mut app, &ctx, size, "Copy diagnostics");
            assert!(app.notice.starts_with("Copied diagnostic"));
            assert!(app.diagnostic_report().contains("DJM-A9"));
            click(&mut app, &ctx, size, "Players");
            assert_eq!(app.deck_compare, compare);
            assert_eq!(app.deck_focus, focus);
        }
    }
}
