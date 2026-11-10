// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

#[test]
fn overview_copies_versions_errors_and_cached_logs_and_preserves_log_pause() {
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
        click(&mut app, &ctx, size, "Setup");
        click(&mut app, &ctx, size, "Diag");
        assert!(!app.diagnostics.showing_logs);
        assert!(
            app.diagnostics.logs.updated.is_none(),
            "overview must not fetch logs"
        );
        click(&mut app, &ctx, size, "Copy diagnostics");
        assert!(app.notice.starts_with("Copied diagnostic"));
        let report = app.diagnostic_report();
        assert!(report.contains(crate::build_info::ID));
        assert!(report.contains(crate::build_info::TARGET));
        assert!(report.contains("Demo · no connected daemon"));
        assert!(report.contains("Loaded tracks: 96"));
        click(&mut app, &ctx, size, "Diag tab Logs");
        click(&mut app, &ctx, size, "Diag pause");
        let logs = app.diagnostics.logs.copy_text();
        click(&mut app, &ctx, size, "Diag tab Status");
        assert!(app.diagnostics.logs.paused);
        assert!(app.diagnostic_report().contains(&logs));
        app.diagnostics
            .requests
            .observe(&Event::Load(Err("Load rejected".into())));
        assert!(
            app.diagnostic_report()
                .contains("API: 1 · Logs: 0 · Version: 0")
        );
        assert!(app.diagnostic_report().contains("Load rejected"));
        // Reconnecting clears diagnostics from the previous server/session.
        app.connect(&ctx);
        assert!(!app.diagnostic_report().contains("Load rejected"));
        assert!(app.diagnostics.logs.entries.is_empty());
        assert!(app.diagnostics.version.info.is_none());
    }
}

#[test]
fn overview_keeps_last_successful_status_age_and_marks_recovery_and_manual_refresh() {
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
    let (worker, _commands, events) = Worker::test();
    app.worker = Some(worker);
    let sampled = Instant::now() - Duration::from_secs(10);
    events
        .send(Event::Status(sampled, Ok((demo::status(), true))))
        .unwrap();
    events
        .send(Event::Status(
            Instant::now(),
            Err("Connection refused".into()),
        ))
        .unwrap();
    events
        .send(Event::Connection(api::Connection::Waiting {
            attempt: 2,
            until: Instant::now() + Duration::from_secs(5),
        }))
        .unwrap();
    app.poll(&ctx);
    assert!(!app.fresh());
    let report = app.diagnostic_report();
    assert!(report.contains("Player update: 10."));
    assert!(report.contains("Retrying · attempt 2"));
    assert!(report.contains("Link: Unknown"));
    assert!(report.contains("Refresh: Waiting for connection"));
    assert!(report.contains("Connection refused"));
    events
        .send(Event::Status(Instant::now(), Ok((demo::status(), true))))
        .unwrap();
    app.poll(&ctx);
    let report = app.diagnostic_report();
    assert!(report.contains("Connected"));
    assert!(report.contains("Refresh: Manual refresh only"));
    assert!(
        report.contains("Last API error"),
        "recovery keeps error history until reconnect"
    );
    app.library_session = Some("test".into());
    app.refresh.tracks_revision = Some(9);
    app.refresh.checked = Some(Instant::now());
    assert!(app.diagnostic_report().contains("checks every 5s"));
    app.refresh.skip_cues = true;
    assert!(app.diagnostic_report().contains("cues need manual Refresh"));
}

#[test]
fn diagnostic_touch_view_fits_wraps_pauses_scrolls_and_resumes_on_both_screens() {
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
        click(&mut app, &ctx, size, "Setup");
        click(&mut app, &ctx, size, "Diag");
        assert_eq!(app.page, Page::Diag);
        assert_eq!(app.navigation_page(), Page::Settings);
        click(&mut app, &ctx, size, "Diag tab Logs");
        frame(&mut app, &ctx, size, vec![]);
        let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
        for label in [
            "Players",
            "Collection",
            "Playlists",
            "Setup",
            "Diag back",
            "Diag pause",
            "Diag latest",
            "Diag copy",
            "Diag logs",
            "Diag status",
        ] {
            let rect = app.hits.iter().find(|(n, _)| n == label).unwrap().1;
            assert!(
                bounds.contains_rect(rect),
                "{label} outside screen: {rect:?}"
            );
            if !matches!(label, "Diag logs" | "Diag status") {
                assert!(rect.width() >= 48.0 && rect.height() >= 48.0);
            }
        }
        assert!(
            app.diagnostics.rows.len() > app.diagnostics.logs.entries.len(),
            "long entries must wrap"
        );
        assert!(app.diagnostics.rows.iter().any(|r| r.color == ORANGE));
        assert_eq!(
            log_color("request failed"),
            Color32::from_rgb(255, 123, 123)
        );
        click(&mut app, &ctx, size, "Diag copy");
        assert!(app.notice.starts_with("Copied 16"));
        click(&mut app, &ctx, size, "Diag pause");
        assert!(app.diagnostics.logs.paused);
        click(&mut app, &ctx, size, "Diag back");
        assert_eq!(app.page, Page::Settings);
        click(&mut app, &ctx, size, "Diag");
        assert!(app.diagnostics.logs.paused);
        click(&mut app, &ctx, size, "Diag pause");
        assert!(!app.diagnostics.logs.paused);

        // Enough history to scroll on either display, with the same contact
        // handling as Collection (no previous-pointer jump on touchdown).
        app.diagnostics.logs.entries = (1..=100)
            .map(|seq| crate::diagnostics::Entry {
                seq,
                time: seq as i64 * 1000,
                line: format!("status: player update {seq}"),
            })
            .collect();
        app.diagnostics.logs.revision += 1;
        frame(&mut app, &ctx, size, vec![]);
        frame(&mut app, &ctx, size, vec![]);
        let view = app.hits.iter().find(|(n, _)| n == "Diag logs").unwrap().1;
        let bottom_offset = app.diagnostics.scroll_offset;
        assert!(bottom_offset > 100.0);
        let start = view.center();
        for (phase, pos) in [
            (TouchPhase::Start, start),
            (TouchPhase::Move, start + vec2(0.0, 66.0)),
            (TouchPhase::End, start + vec2(0.0, 66.0)),
        ] {
            let mut events = vec![
                eframe::egui::Event::PointerMoved(pos),
                eframe::egui::Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase,
                    pos,
                    force: None,
                },
            ];
            if phase != TouchPhase::Move {
                events.push(eframe::egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: phase == TouchPhase::Start,
                    modifiers: Modifiers::NONE,
                });
            }
            frame(&mut app, &ctx, size, events);
            assert!(app.diagnostics.logs.paused);
            let expected = if phase == TouchPhase::Start {
                bottom_offset
            } else {
                bottom_offset - 66.0
            };
            assert!(
                (app.diagnostics.scroll_offset - expected).abs() < 1.0,
                "{phase:?}: expected {expected}, got {}",
                app.diagnostics.scroll_offset
            );
        }
        click(&mut app, &ctx, size, "Diag latest");
        assert!(!app.diagnostics.logs.paused);
        assert!((app.diagnostics.scroll_offset - bottom_offset).abs() < 1.0);
        click(&mut app, &ctx, size, "Collection");
        app.connect(&ctx);
        assert!(
            app.diagnostics.logs.entries.is_empty(),
            "reconnect clears old server logs"
        );
        click(&mut app, &ctx, size, "Setup");
        click(&mut app, &ctx, size, "Diag");
        click(&mut app, &ctx, size, "Diag tab Logs");
        app.diagnostics.logs.error = "This Vynull server does not provide the log endpoint.".into();
        frame(&mut app, &ctx, size, vec![]);
        let status = app.hits.iter().find(|(n, _)| n == "Diag status").unwrap().1;
        assert!(bounds.contains_rect(status));
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
        assert_eq!(app.page, Page::Settings);
    }
}
