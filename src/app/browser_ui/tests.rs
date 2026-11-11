// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click as click_once, frame};

fn click(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    click_once(app, ctx, size, label);
    // Let newly opened modal areas finish their sizing/positioning passes.
    frame(app, ctx, size, vec![]);
    frame(app, ctx, size, vec![]);
}

fn app(ctx: &Context) -> App {
    App::new(
        ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    )
}

#[test]
fn browser_touch_layout_edits_menus_and_detail_at_all_display_sizes() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let ctx = Context::default();
        let mut app = app(&ctx);
        app.page = Page::Settings;
        click(&mut app, &ctx, size, "CDJ & mixer");
        click(&mut app, &ctx, size, "Device tab Browser");
        frame(&mut app, &ctx, size, vec![]);
        let bounds = Rect::from_min_max(pos2(88.0, 52.0), size.to_pos2() - vec2(0.0, 26.0));
        for name in [
            "Browser save",
            "Browser discard",
            "Browser reload",
            "Browser Categories",
            "Browser Sort menu",
            "Browser detail",
            "Browser toggle artist",
            "Browser move artist 1",
        ] {
            let rect = app.hits.iter().find(|(n, _)| n == name).unwrap().1;
            assert!(bounds.contains_rect(rect), "{name}: {rect:?}");
            assert!(rect.height() >= 48.0);
        }
        let first = app
            .hits
            .iter()
            .find(|(n, _)| n == "Browser move artist 1")
            .unwrap()
            .1;
        for (_, rect) in app
            .hits
            .iter()
            .filter(|(n, _)| n.starts_with("Browser move ") && n.ends_with(" 1"))
        {
            assert!(
                (first.left() - rect.left()).abs() < 1.0,
                "row controls drift horizontally: {rect:?}"
            );
            assert!(
                rect.right() <= size.x - 16.0,
                "row control clipped: {rect:?}"
            );
        }
        click(&mut app, &ctx, size, "Browser move album -1");
        assert_eq!(
            app.browser_settings.draft.as_ref().unwrap().items[0].key,
            "album"
        );
        click(&mut app, &ctx, size, "Browser toggle album");
        assert!(!app.browser_settings.draft.as_ref().unwrap().items[0].visible);
        click(&mut app, &ctx, size, "Browser detail");
        click(&mut app, &ctx, size, "Browser detail artist");
        assert_eq!(
            app.browser_settings.draft.as_ref().unwrap().track_detail,
            "artist"
        );
        click(&mut app, &ctx, size, "Browser Sort menu");
        click(&mut app, &ctx, size, "Browser move 1 -1");
        assert_eq!(app.browser_settings.draft.as_ref().unwrap().sorts[0].id, 1);
        click(&mut app, &ctx, size, "Browser toggle 1");
        click(&mut app, &ctx, size, "Browser save");
        assert!(!app.browser_settings.dirty());
        let saved = app.browser_settings.snapshot.as_ref().unwrap();
        assert_eq!(saved.track_detail, "artist");
        assert!(!saved.items[0].visible);
        assert!(!saved.sorts[0].visible);
        click(&mut app, &ctx, size, "Browser detail");
        click(&mut app, &ctx, size, "Browser detail bpm");
        click(&mut app, &ctx, size, "Browser discard");
        assert_eq!(
            app.browser_settings.draft.as_ref().unwrap().track_detail,
            "artist"
        );
    }
}

fn touch(app: &mut App, ctx: &Context, size: Vec2, pos: Pos2, phase: TouchPhase) {
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
    if matches!(phase, TouchPhase::Start | TouchPhase::End) {
        events.push(eframe::egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: phase == TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    frame(app, ctx, size, events);
}

#[test]
fn browser_touch_scroll_does_not_toggle_rows_and_taps_do_not_jump() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.open_device_settings();
    click(&mut app, &ctx, size, "Device tab Browser");
    let rect = |app: &App, name: &str| app.hits.iter().find(|(n, _)| n == name).unwrap().1;
    let artist = rect(&app, "Browser toggle artist");
    let pos = pos2(280.0, artist.center().y);
    touch(&mut app, &ctx, size, pos, TouchPhase::Start);
    touch(&mut app, &ctx, size, pos, TouchPhase::End);
    assert_eq!(rect(&app, "Browser toggle artist"), artist);
    assert!(!app.browser_settings.dirty());
    let start = rect(&app, "Browser toggle album").center();
    touch(&mut app, &ctx, size, start, TouchPhase::Start);
    for step in 1..=4 {
        touch(
            &mut app,
            &ctx,
            size,
            start - vec2(0.0, step as f32 * 20.0),
            TouchPhase::Move,
        );
    }
    touch(
        &mut app,
        &ctx,
        size,
        start - vec2(0.0, 80.0),
        TouchPhase::End,
    );
    assert!(
        !app.browser_settings.dirty(),
        "dragging over buttons must not edit the menu"
    );
    let album = rect(&app, "Browser toggle album");
    assert!(album.top() < start.y - 40.0, "the list must scroll");
    touch(&mut app, &ctx, size, album.center(), TouchPhase::Start);
    assert_eq!(rect(&app, "Browser toggle album"), album);
    touch(&mut app, &ctx, size, album.center(), TouchPhase::End);
    assert!(
        !app.browser_settings
            .draft
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|v| v.key == "album")
            .unwrap()
            .visible
    );
    // The detail chooser also permits a drag without accidentally picking a field.
    click(&mut app, &ctx, size, "Browser detail");
    let draft = app.browser_settings.draft.clone();
    let start = rect(&app, "Browser detail genre").center();
    touch(&mut app, &ctx, size, start, TouchPhase::Start);
    for step in 1..=8 {
        touch(
            &mut app,
            &ctx,
            size,
            start - vec2(0.0, step as f32 * 18.0),
            TouchPhase::Move,
        );
    }
    touch(
        &mut app,
        &ctx,
        size,
        start - vec2(0.0, 144.0),
        TouchPhase::End,
    );
    assert!(app.browser_settings.picker);
    assert_eq!(app.browser_settings.draft, draft);
    for _ in 0..30 {
        frame(&mut app, &ctx, size, vec![]);
    }
    assert!(rect(&app, "Browser detail bpm").bottom() < 112.0);
}

#[test]
fn browser_and_hardware_drafts_survive_navigation_and_save_independently() {
    let ctx = Context::default();
    let size = vec2(800.0, 480.0);
    let mut app = app(&ctx);
    app.open_device_settings();
    click(&mut app, &ctx, size, "Device my_setting.auto_cue");
    click(&mut app, &ctx, size, "Device choice off");
    click(&mut app, &ctx, size, "Device tab Browser");
    click(&mut app, &ctx, size, "Browser detail");
    click(&mut app, &ctx, size, "Browser detail artist");
    click(&mut app, &ctx, size, "Device settings back");
    click(&mut app, &ctx, size, "CDJ & mixer");
    assert!(app.browser_settings.dirty());
    click(&mut app, &ctx, size, "Browser save");
    click(&mut app, &ctx, size, "Device tab CDJ");
    click(&mut app, &ctx, size, "Device save");
    click(&mut app, &ctx, size, "Device tab Browser");
    assert_eq!(
        app.browser_settings.snapshot.as_ref().unwrap().track_detail,
        "artist"
    );
}

#[test]
fn browser_failure_blocks_retry_stale_results_and_server_switch_while_pending() {
    let ctx = Context::default();
    let size = vec2(800.0, 480.0);
    let mut app = app(&ctx);
    app.open_device_settings();
    click(&mut app, &ctx, size, "Device tab Browser");
    app.browser_settings.draft.as_mut().unwrap().track_detail = "artist".into();
    let token = app.browser_settings.serial;
    app.browser_settings_response(token + 1, true, Ok(Snapshot::demo()));
    assert!(app.browser_settings.dirty());
    app.browser_settings_response(token, true, Err("Uncertain write; reload".into()));
    click(&mut app, &ctx, size, "Browser save");
    assert!(app.browser_settings.reload && app.browser_settings.dirty());
    click(&mut app, &ctx, size, "Browser reload");
    assert!(!app.browser_settings.reload && !app.browser_settings.dirty());
    app.browser_settings.pending = true;
    app.page = Page::Settings;
    app.server_draft = "http://another.local:9443".into();
    click(&mut app, &ctx, size, "Server connect");
    assert!(app.options.demo);
    app.connect(&ctx);
    assert!(app.browser_settings.draft.is_none());
}
