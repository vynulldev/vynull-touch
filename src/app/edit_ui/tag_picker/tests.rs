// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

fn setup(ctx: &Context) -> App {
    let mut app = App::new(
        ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    app.preview = Controller::test().0;
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
    app.demo_edits
        .run(
            &Request::DeleteCategory {
                before: Category {
                    id: 1,
                    name: "Collection".into(),
                },
                tags: vec![],
            },
            &app.all_tracks,
            &app.playlists,
        )
        .unwrap();
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
    app.open_track_edits(app.all_tracks[0].clone());
    app.editor.as_mut().unwrap().view = View::TrackTags;
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

fn selected(app: &App) -> BTreeSet<u32> {
    app.editor
        .as_ref()
        .unwrap()
        .track
        .as_ref()
        .unwrap()
        .tags
        .clone()
}

#[test]
fn filtered_tag_selection_preserves_hidden_ids_saves_and_cancels_at_all_screen_sizes() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let ctx = Context::default();
        let mut app = setup(&ctx);
        settle(&mut app, &ctx, size);
        let footer = hit(&app, "Edit Save track");
        assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(footer));
        assert!(footer.top() > hit(&app, "Track tag 1").bottom());
        assert!(hit(&app, "Track tag 1").height() >= 48.0);
        click(&mut app, &ctx, size, "Track tag 2");
        click(&mut app, &ctx, size, "Track tag 3"); // Same name, different category/ID.
        click(&mut app, &ctx, size, "Tag category selected");
        settle(&mut app, &ctx, size);
        assert_eq!(selected(&app), BTreeSet::from([1, 2, 3]));
        assert_eq!(
            app.hits
                .iter()
                .filter(|(name, _)| name.starts_with("Track tag "))
                .count(),
            3
        );
        click(&mut app, &ctx, size, "Track tag 2");
        assert_eq!(selected(&app), BTreeSet::from([1, 3]));
        click(&mut app, &ctx, size, "Tag category 2");
        app.editor.as_mut().unwrap().query = "sHaReD".into();
        settle(&mut app, &ctx, size);
        assert!(app.hits.iter().any(|(name, _)| name == "Track tag 2"));
        assert!(!app.hits.iter().any(|(name, _)| name == "Track tag 3"));
        click(&mut app, &ctx, size, "Track tag 2");
        click(&mut app, &ctx, size, "Edit Save track");
        assert_eq!(selected(&app), BTreeSet::from([1, 2, 3]));
        assert!(
            !app.editor
                .as_ref()
                .unwrap()
                .track
                .as_ref()
                .unwrap()
                .changed()
        );
        assert_eq!(
            app.all_tracks[0]
                .tags
                .iter()
                .filter(|tag| *tag == "Shared")
                .count(),
            2
        );
        click(&mut app, &ctx, size, "Tag category selected");
        click(&mut app, &ctx, size, "Track tag 3");
        click(&mut app, &ctx, size, "Edit Cancel edits");
        assert_eq!(selected(&app), BTreeSet::from([1, 2, 3]));
        click(&mut app, &ctx, size, "Tag category all");
        app.editor.as_mut().unwrap().query = "mOoD".into();
        settle(&mut app, &ctx, size);
        assert!(app.hits.iter().any(|(name, _)| name == "Track tag 3"));
        assert!(!app.hits.iter().any(|(name, _)| name == "Track tag 2"));
        app.editor.as_mut().unwrap().query = "no matching tags".into();
        settle(&mut app, &ctx, size);
        assert!(
            !app.hits
                .iter()
                .any(|(name, _)| name.starts_with("Track tag "))
        );
        assert_eq!(hit(&app, "Edit Save track"), footer);
        click(&mut app, &ctx, size, "Edit Clear search");
        assert_eq!(selected(&app), BTreeSet::from([1, 2, 3]));
        assert!(!app.preview.request.playing);
    }
}

#[test]
fn touch_scroll_keeps_tag_drafts_and_footer_stable_and_new_taps_select_the_visible_tile() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = setup(&ctx);
    settle(&mut app, &ctx, size);
    let saved = selected(&app);
    let footer = hit(&app, "Edit Save track");
    let start = hit(&app, "Track tag 1").center();
    for (phase, pos) in [
        (TouchPhase::Start, start),
        (TouchPhase::Move, start - vec2(0.0, 24.0)),
        (TouchPhase::Move, start - vec2(0.0, 100.0)),
        (TouchPhase::End, start - vec2(0.0, 100.0)),
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
        if phase == TouchPhase::End {
            events.push(eframe::egui::Event::PointerGone);
        }
        frame(&mut app, &ctx, size, events);
    }
    settle(&mut app, &ctx, size);
    assert_eq!(selected(&app), saved, "dragging must not toggle a tag");
    assert_eq!(hit(&app, "Edit Save track"), footer);
    assert!(!app.hits.iter().any(|(name, _)| name == "Track tag 1"));
    let label = app
        .hits
        .iter()
        .find(|(name, rect)| {
            name.starts_with("Track tag ")
                && rect.top() > 215.0
                && rect.bottom() < footer.top() - 8.0
        })
        .unwrap()
        .0
        .clone();
    let id: u32 = label.strip_prefix("Track tag ").unwrap().parse().unwrap();
    // Touch down directly on the last drawn tile: unlike a mouse, a new finger
    // has no hover frames while the list is coasting.
    let pos = hit(&app, &label).center();
    for (phase, pressed) in [(TouchPhase::Start, true), (TouchPhase::End, false)] {
        frame(
            &mut app,
            &ctx,
            size,
            vec![
                eframe::egui::Event::PointerMoved(pos),
                eframe::egui::Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(2),
                    phase,
                    pos,
                    force: None,
                },
                eframe::egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    assert!(selected(&app).contains(&id));
    assert_eq!(selected(&app).len(), saved.len() + 1);
}
