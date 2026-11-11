// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::frame;
use eframe::egui::Event as InputEvent;

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    frame(app, ctx, size, vec![]);
    let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
    assert!(
        Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
        "{label}: {rect:?}"
    );
    let pos = rect.center();
    for phase in [TouchPhase::Start, TouchPhase::End] {
        let mut events = vec![
            InputEvent::PointerMoved(pos),
            InputEvent::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: phase == TouchPhase::Start,
                modifiers: Modifiers::NONE,
            },
            InputEvent::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(1),
                phase,
                pos,
                force: None,
            },
        ];
        if phase == TouchPhase::End {
            events.push(InputEvent::PointerGone);
        }
        frame(app, ctx, size, events);
    }
    // Let centered modals settle after opening or changing content.
    frame(app, ctx, size, vec![]);
    frame(app, ctx, size, vec![]);
}

#[test]
fn touch_defaults_fit_save_restore_and_feed_the_cue_editor() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let root = std::env::temp_dir().join(format!(
            "vynull-cue-colors-ui-{}-{}",
            std::process::id(),
            size.x
        ));
        let path = root.join("settings.json");
        settings::save_hot_cue_colors(&path, [0; 8]).unwrap();
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            Some(path.clone()),
        );
        app.preview = Controller::test().0;
        app.page = Page::Settings;
        tap(&mut app, &ctx, size, "Cue defaults");
        assert!(app.cue_defaults.is_some());
        for (name, rect) in &app.hits {
            if name.starts_with("Default cue") || name == "Cue defaults done" {
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains_rect(*rect),
                    "{name}: {rect:?}"
                );
                assert!(rect.width() >= 48.0 && rect.height() >= 48.0);
            }
        }
        let mut colors = [49, 56, 60, 62, 1, 5, 9, 14];
        for (index, id) in colors.iter().enumerate() {
            tap(
                &mut app,
                &ctx,
                size,
                &format!("Default cue slot {}", cues::slot(index as u16 + 1)),
            );
            tap(&mut app, &ctx, size, &format!("Default cue color {id}"));
            assert_eq!(app.hot_cue_colors[index], *id);
        }
        tap(&mut app, &ctx, size, "Default cue slot A");
        tap(&mut app, &ctx, size, "Default cue color 0");
        colors[0] = 0;
        tap(&mut app, &ctx, size, "Cue defaults done");
        assert!(app.cue_defaults.is_none());
        assert_eq!(settings::load(Some(&path)).hot_cue_colors, colors);
        let restored = App::new(
            &Context::default(),
            Options {
                demo: true,
                ..Default::default()
            },
            Some(path),
        );
        assert_eq!(restored.hot_cue_colors, colors);

        app.inspect(app.tracks[0].clone());
        let existing = app.detail.as_ref().unwrap().cues.clone();
        tap(&mut app, &ctx, size, "Edit cues");
        assert_eq!(app.cue_editor.as_ref().unwrap().draft.color_id, colors[4]);
        tap(&mut app, &ctx, size, "Edit slot 7");
        assert_eq!(app.cue_editor.as_ref().unwrap().draft.color_id, colors[6]);
        tap(&mut app, &ctx, size, "Cue color 42");
        tap(&mut app, &ctx, size, "Cue save");
        assert!(app.cue_editor.is_none());
        for cue in &existing {
            assert!(app.detail.as_ref().unwrap().cues.contains(cue));
        }
        tap(&mut app, &ctx, size, "Edit cues");
        tap(&mut app, &ctx, size, "Edit slot 7");
        assert_eq!(app.cue_editor.as_ref().unwrap().draft.color_id, 42);
        tap(&mut app, &ctx, size, "Cue cancel");
        assert_eq!(app.hot_cue_colors, colors);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn failed_default_save_keeps_previous_colors_and_escape_closes_picker() {
    let size = vec2(800.0, 480.0);
    let root =
        std::env::temp_dir().join(format!("vynull-cue-colors-failure-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let blocker = root.join("file");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let ctx = Context::default();
    let mut app = App::new(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    app.settings_path = Some(blocker.join("settings.json"));
    app.page = Page::Settings;
    tap(&mut app, &ctx, size, "Cue defaults");
    tap(&mut app, &ctx, size, "Default cue color 49");
    assert_eq!(app.hot_cue_colors, [0; 8]);
    assert!(
        app.cue_defaults
            .as_ref()
            .unwrap()
            .error
            .starts_with("Could not save cue colors:")
    );
    frame(
        &mut app,
        &ctx,
        size,
        vec![InputEvent::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(app.cue_defaults.is_none());
    assert_eq!(app.page, Page::Settings);
    std::fs::remove_dir_all(root).unwrap();
}
