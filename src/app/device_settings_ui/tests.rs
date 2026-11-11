// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

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
fn hardware_setting_touch_taps_select_and_dragging_long_choices_does_not_change_the_draft() {
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
    app.open_device_settings();
    frame(&mut app, &ctx, size, vec![]);
    frame(&mut app, &ctx, size, vec![]);
    let pos = app
        .hits
        .iter()
        .find(|(label, _)| label == "Device my_setting.auto_cue")
        .unwrap()
        .1
        .center();
    touch(&mut app, &ctx, size, pos, TouchPhase::Start);
    touch(&mut app, &ctx, size, pos, TouchPhase::End);
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.device_settings.picker.is_some());
    let pos = app
        .hits
        .iter()
        .find(|(label, _)| label == "Device choice off")
        .unwrap()
        .1
        .center();
    touch(&mut app, &ctx, size, pos, TouchPhase::Start);
    touch(&mut app, &ctx, size, pos, TouchPhase::End);
    assert_eq!(app.device_settings.draft["my_setting"]["auto_cue"], "off");
    assert!(app.device_settings.picker.is_none());
    app.device_settings.picker = Some(("djm_my_setting".into(), "midi_channel".into()));
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![]);
    }
    let draft = app.device_settings.draft.clone();
    let start = pos2(400.0, 290.0);
    touch(&mut app, &ctx, size, start, TouchPhase::Start);
    for i in 1..=8 {
        touch(
            &mut app,
            &ctx,
            size,
            start - vec2(0.0, i as f32 * 18.0),
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
    assert!(app.device_settings.picker.is_some());
    assert_eq!(app.device_settings.draft, draft);
    for _ in 0..30 {
        frame(&mut app, &ctx, size, vec![]);
    }
    let scrolled_one = app
        .hits
        .iter()
        .find(|(label, _)| label == "Device choice one")
        .unwrap()
        .1;
    assert!(
        scrolled_one.bottom() < 100.0,
        "long list did not scroll: {scrolled_one:?}"
    );
}

#[test]
fn hardware_settings_fit_all_displays_edit_save_discard_and_keep_local_preferences() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            None,
        );
        let local_style = app.waveform_style;
        app.page = Page::Settings;
        click(&mut app, &ctx, size, "CDJ & mixer");
        assert_eq!(app.page, Page::DeviceSettings);
        assert_eq!(app.navigation_page(), Page::Settings);
        frame(&mut app, &ctx, size, vec![]);
        let bounds = Rect::from_min_size(Pos2::ZERO, size - vec2(0.0, 26.0));
        let first = app
            .hits
            .iter()
            .find(|(n, _)| n == "Device my_setting.auto_cue")
            .unwrap()
            .1;
        let second = app
            .hits
            .iter()
            .find(|(n, _)| n == "Device my_setting.auto_cue_level")
            .unwrap()
            .1;
        assert!((first.left() - second.left()).abs() < 1.0);
        assert!((first.right() - second.right()).abs() < 1.0);
        assert!(
            first.right() > size.x - 42.0,
            "setting rows must fill the content width"
        );
        for name in [
            "Device save",
            "Device discard",
            "Device reload",
            "Device settings back",
            "Device tab Mixer",
        ] {
            let rect = app.hits.iter().find(|(n, _)| n == name).unwrap().1;
            assert!(bounds.contains_rect(rect), "{name}: {rect:?}");
        }
        click(&mut app, &ctx, size, "Device my_setting.auto_cue");
        click(&mut app, &ctx, size, "Device choice off");
        assert!(app.device_settings.dirty());
        assert_eq!(
            app.device_settings.snapshot.as_ref().unwrap().full["my_setting"]["auto_cue"],
            "on"
        );
        click(&mut app, &ctx, size, "Device settings back");
        click(&mut app, &ctx, size, "CDJ & mixer");
        assert!(
            app.device_settings.dirty(),
            "Back must preserve an unsaved draft"
        );
        click(&mut app, &ctx, size, "Device save");
        assert!(!app.device_settings.dirty());
        assert_eq!(
            app.device_settings.snapshot.as_ref().unwrap().full["my_setting"]["auto_cue"],
            "off"
        );
        click(&mut app, &ctx, size, "Device tab Display");
        click(&mut app, &ctx, size, "Device dev_setting.key_display");
        click(&mut app, &ctx, size, "Device choice classic");
        click(&mut app, &ctx, size, "Device discard");
        assert_eq!(
            app.device_settings.draft["dev_setting"]["key_display"],
            "alphanumeric"
        );
        click(&mut app, &ctx, size, "Device tab Mixer");
        click(
            &mut app,
            &ctx,
            size,
            "Device djm_my_setting.beat_fx_quantize",
        );
        click(&mut app, &ctx, size, "Device choice off");
        click(&mut app, &ctx, size, "Device save");
        assert_eq!(
            app.device_settings.snapshot.as_ref().unwrap().full["djm_my_setting"]["beat_fx_quantize"],
            "off"
        );
        assert_eq!(app.waveform_style, local_style);
    }
}

#[test]
fn device_settings_failed_save_requires_reload_and_stale_reply_is_ignored() {
    let ctx = Context::default();
    let mut app = App::new(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    app.open_device_settings();
    app.device_settings.draft["my_setting"]["auto_cue"] = serde_json::json!("off");
    let serial = app.device_settings.serial;
    app.device_settings_response(serial + 1, true, Ok(Snapshot::demo()));
    assert!(app.device_settings.dirty());
    app.device_settings_response(serial, true, Err("Save outcome unknown. Reload.".into()));
    app.device_settings_request(true);
    assert!(app.device_settings.dirty());
    assert!(app.device_settings.reload);
    app.device_settings_request(false);
    assert!(!app.device_settings.dirty());
    assert!(!app.device_settings.reload);
    app.device_settings.pending = true;
    app.device_settings_request(false);
    assert_eq!(app.device_settings.serial, serial + 1);
}
