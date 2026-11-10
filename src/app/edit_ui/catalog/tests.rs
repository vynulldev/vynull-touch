// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::tests::{app, click};
use super::*;
use crate::app::tests::frame;
use eframe::egui::Event;

fn setup(ctx: &Context) -> App {
    let mut app = app(ctx);
    for name in ["Energy", "Mood"] {
        app.demo_edits
            .run(
                &Request::Category {
                    before: None,
                    name: name.into(),
                },
                &app.all_tracks,
                &app.playlists,
            )
            .unwrap();
    }
    for i in 0..48 {
        app.demo_edits
            .run(
                &Request::Tag {
                    before: None,
                    name: if i < 2 {
                        "Shared".into()
                    } else {
                        format!("Tag {i:02}")
                    },
                    category: Some(Category {
                        id: if i % 2 == 0 { 2 } else { 3 },
                        name: if i % 2 == 0 { "Energy" } else { "Mood" }.into(),
                    }),
                },
                &app.all_tracks,
                &app.playlists,
            )
            .unwrap();
    }
    app.open_tag_manager();
    app
}

fn settle(app: &mut App, ctx: &Context, size: Vec2) {
    for _ in 0..3 {
        frame(app, ctx, size, vec![]);
    }
}

fn hit(app: &App, name: &str) -> Rect {
    app.hits
        .iter()
        .find(|(label, _)| label == name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .1
}

#[test]
fn catalog_browsing_keeps_identity_and_category_context_across_filters_and_edits() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let ctx = Context::default();
        let mut app = setup(&ctx);
        settle(&mut app, &ctx, size);
        let footer = hit(&app, "Edit New tag");
        assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(footer));
        assert!(hit(&app, "Manage tag 1").height() >= 48.0);
        click(&mut app, &ctx, size, "Manage scope 2");
        app.editor.as_mut().unwrap().query = "sHaReD".into();
        settle(&mut app, &ctx, size);
        assert!(app.hits.iter().any(|(name, _)| name == "Manage tag 2"));
        assert!(!app.hits.iter().any(|(name, _)| name == "Manage tag 3"));
        click(&mut app, &ctx, size, "Manage tag 2");
        let View::TagForm {
            before,
            name,
            category,
        } = &mut app.editor.as_mut().unwrap().view
        else {
            panic!()
        };
        assert_eq!(before.as_ref().unwrap().id, 2);
        assert_eq!(*category, 2);
        *name = "Discard this".into();
        click(&mut app, &ctx, size, "Edit Cancel");
        assert_eq!(app.editor.as_ref().unwrap().query, "sHaReD");
        assert_eq!(app.editor.as_ref().unwrap().catalog.tags[1].name, "Shared");
        click(&mut app, &ctx, size, "Edit Edit category…");
        click(&mut app, &ctx, size, "Edit Cancel");
        assert!(matches!(app.editor.as_ref().unwrap().view, View::Tags));
        assert_eq!(app.editor.as_ref().unwrap().query, "sHaReD");
        click(&mut app, &ctx, size, "Edit New tag");
        let View::TagForm {
            before,
            name,
            category,
        } = &mut app.editor.as_mut().unwrap().view
        else {
            panic!()
        };
        assert!(before.is_none());
        assert_eq!(*category, 2, "new tags inherit the browsed category");
        *name = "New energy tag".into();
        click(&mut app, &ctx, size, "Edit Save tag");
        let added = app
            .editor
            .as_ref()
            .unwrap()
            .catalog
            .tags
            .iter()
            .find(|t| t.name == "New energy tag")
            .unwrap()
            .clone();
        assert_eq!(added.category_id, 2);
        click(&mut app, &ctx, size, &format!("Manage tag {}", added.id));
        click(&mut app, &ctx, size, "Tag form category 3");
        click(&mut app, &ctx, size, "Edit Save tag");
        assert_eq!(
            app.editor
                .as_ref()
                .unwrap()
                .catalog
                .tags
                .iter()
                .find(|t| t.id == added.id)
                .unwrap()
                .category_id,
            3
        );
        click(&mut app, &ctx, size, "Manage scope all");
        app.editor.as_mut().unwrap().query = "mOoD".into();
        settle(&mut app, &ctx, size);
        assert!(app.hits.iter().any(|(name, _)| name == "Manage tag 3"));
        assert!(!app.hits.iter().any(|(name, _)| name == "Manage tag 2"));
        app.editor.as_mut().unwrap().query = "no matching tags".repeat(8);
        settle(&mut app, &ctx, size);
        assert!(
            !app.hits
                .iter()
                .any(|(name, _)| name.starts_with("Manage tag "))
        );
        assert_eq!(hit(&app, "Edit New tag"), footer);
        click(&mut app, &ctx, size, "Edit Clear search");
        click(&mut app, &ctx, size, "Edit Categories");
        app.editor.as_mut().unwrap().query = "energy".into();
        settle(&mut app, &ctx, size);
        assert!(app.hits.iter().any(|(name, _)| name == "Manage category 2"));
        assert!(!app.hits.iter().any(|(name, _)| name == "Manage category 3"));
        click(&mut app, &ctx, size, "Manage category 2");
        settle(&mut app, &ctx, size);
        for label in ["Edit Save category", "Edit Cancel", "Edit Delete category…"] {
            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(hit(&app, label)));
        }
        click(&mut app, &ctx, size, "Edit Cancel");
        click(&mut app, &ctx, size, "Edit Clear search");
        click(&mut app, &ctx, size, "Manage category 0");
        assert!(matches!(app.editor.as_ref().unwrap().view, View::Tags));
        assert_eq!(
            app.editor.as_ref().unwrap().catalog_browser.category,
            Some(0)
        );
        settle(&mut app, &ctx, size);
        assert!(
            !app.hits
                .iter()
                .any(|(name, _)| name == "Edit Edit category…")
        );
        click(&mut app, &ctx, size, "Edit New tag");
        assert!(matches!(
            app.editor.as_ref().unwrap().view,
            View::TagForm { category: 0, .. }
        ));
    }
}

fn touch(app: &mut App, ctx: &Context, size: Vec2, phase: TouchPhase, pos: Pos2) {
    let mut events = vec![
        Event::PointerMoved(pos),
        Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        },
    ];
    if phase != TouchPhase::Move {
        events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: phase == TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    if phase == TouchPhase::End {
        events.push(Event::PointerGone);
    }
    frame(app, ctx, size, events);
}

#[test]
fn catalog_touch_scrolling_opens_the_tapped_tag_and_keeps_form_actions_visible() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = setup(&ctx);
    settle(&mut app, &ctx, size);
    let footer = hit(&app, "Edit New tag");
    let start = hit(&app, "Manage tag 1").center();
    for (phase, offset) in [
        (TouchPhase::Start, 0.0),
        (TouchPhase::Move, 24.0),
        (TouchPhase::Move, 100.0),
        (TouchPhase::End, 100.0),
    ] {
        touch(&mut app, &ctx, size, phase, start - vec2(0.0, offset));
    }
    settle(&mut app, &ctx, size);
    assert!(matches!(app.editor.as_ref().unwrap().view, View::Tags));
    assert_eq!(hit(&app, "Edit New tag"), footer);
    let (label, rect) = app
        .hits
        .iter()
        .find(|(name, rect)| {
            name.starts_with("Manage tag ")
                && rect.top() > 160.0
                && rect.bottom() < footer.top() - 8.0
        })
        .unwrap()
        .clone();
    let id: u32 = label.strip_prefix("Manage tag ").unwrap().parse().unwrap();
    touch(&mut app, &ctx, size, TouchPhase::Start, rect.center());
    touch(&mut app, &ctx, size, TouchPhase::End, rect.center());
    assert!(
        matches!(&app.editor.as_ref().unwrap().view, View::TagForm { before: Some(tag), .. } if tag.id == id)
    );
    // Enough categories to require scrolling on the edit form.
    let manager = app.editor.as_mut().unwrap();
    for id in 4..40 {
        manager.catalog.categories.push(Category {
            id,
            name: format!("Category {id:02}"),
        });
    }
    settle(&mut app, &ctx, size);
    let footer = hit(&app, "Edit Save tag");
    let category = match app.editor.as_ref().unwrap().view {
        View::TagForm { category, .. } => category,
        _ => panic!(),
    };
    let start = app
        .hits
        .iter()
        .find(|(name, rect)| {
            name.starts_with("Tag form category ")
                && rect.top() > 178.0
                && rect.bottom() < footer.top() - 8.0
        })
        .unwrap()
        .1
        .center();
    for (phase, offset) in [
        (TouchPhase::Start, 0.0),
        (TouchPhase::Move, 30.0),
        (TouchPhase::Move, 90.0),
        (TouchPhase::End, 90.0),
    ] {
        touch(&mut app, &ctx, size, phase, start - vec2(0.0, offset));
    }
    settle(&mut app, &ctx, size);
    assert!(
        matches!(app.editor.as_ref().unwrap().view, View::TagForm { category: actual, .. } if actual == category)
    );
    assert_eq!(hit(&app, "Edit Save tag"), footer);
    assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(footer));
}
