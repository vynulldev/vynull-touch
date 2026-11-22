// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

#[test]
fn switching_theme_preserves_state_and_updates_layout_at_touch_sizes() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let path = std::env::temp_dir()
            .join(format!("vynull-theme-ui-{}-{}", std::process::id(), size.x))
            .join("settings.json");
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            Some(path.clone()),
        );
        app.page = Page::Collection;
        app.selected = Some(app.tracks[0].id);
        let selected = app.selected;
        let pair = app.deck_pair;
        let zoom = app.zoom;
        frame(&mut app, &ctx, size, vec![]);
        click(&mut app, &ctx, size, "Setup");
        click(&mut app, &ctx, size, "Theme");
        for theme in [theme::UiTheme::Classic, theme::UiTheme::XdjAz] {
            click(&mut app, &ctx, size, &format!("Theme {}", theme.label()));
            assert_eq!(app.ui_theme, theme);
            assert_eq!(settings::load(Some(&path)).theme, theme);
            let restored_ctx = Context::default();
            let restored = App::new(
                &restored_ctx,
                Options {
                    demo: true,
                    ..Default::default()
                },
                Some(path.clone()),
            );
            assert_eq!(restored.ui_theme, theme);
            assert_eq!(theme::current(&restored_ctx), theme);
            assert_eq!(theme::current(&ctx), theme);
            assert_eq!(
                ctx.style_of(Theme::Dark).visuals.panel_fill,
                theme.palette().bg
            );
            click(&mut app, &ctx, size, "Theme done");
            click(&mut app, &ctx, size, "Collection");
            frame(&mut app, &ctx, size, vec![]);
            assert!(app.hits.iter().any(|(name, _)| name == "Browse deck 1"));
            assert_eq!(app.selected, selected);
            assert_eq!(app.deck_pair, pair);
            assert_eq!(app.zoom, zoom);
            click(&mut app, &ctx, size, "Players");
            frame(&mut app, &ctx, size, vec![]);
            for label in [
                "Deck compare",
                "Deck pair",
                "Compare zoom −",
                "Compare zoom +",
                "Match a player",
                "Compare overview 1",
                "Compare overview 2",
            ] {
                let rect = app
                    .hits
                    .iter()
                    .find(|(name, _)| name == label)
                    .map(|(_, rect)| rect)
                    .unwrap_or_else(|| panic!("missing {label}"));
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains_rect(*rect),
                    "{theme:?} {size:?}: {label}: {rect:?}"
                );
            }
            let controls = app
                .hits
                .iter()
                .find(|(name, _)| name == "Deck compare")
                .unwrap()
                .1;
            assert_eq!(controls.top() < 52.0, theme.is_xdj());
            if !theme.is_xdj() {
                for number in [1, 2] {
                    let lane = app
                        .hits
                        .iter()
                        .find(|(name, _)| name == &format!("Compare player {number}"))
                        .unwrap()
                        .1;
                    let pitch = app
                        .hits
                        .iter()
                        .find(|(name, _)| name == &format!("Compare tempo/key {number}"))
                        .unwrap()
                        .1;
                    assert!(lane.contains_rect(pitch));
                }
            }
            click(&mut app, &ctx, size, "Compare title 1");
            assert_eq!(app.page, Page::Detail);
            click(&mut app, &ctx, size, "Back");
            assert_eq!(app.page, Page::Players);
            click(&mut app, &ctx, size, "Deck compare");
            frame(&mut app, &ctx, size, vec![]);
            assert!(!app.deck_compare);
            for label in [
                "Deck compare",
                "Deck pair",
                "Deck inspect 1",
                "Deck inspect 2",
            ] {
                let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
                    "{theme:?} {label}: {rect:?}"
                );
            }
            click(&mut app, &ctx, size, "Deck compare");
            click(&mut app, &ctx, size, "Setup");
            click(&mut app, &ctx, size, "Theme");
        }
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
