// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::frame;
use crate::waveform::{Overview, Style};

fn flags(app: &mut App, ctx: &Context, size: Vec2, waveform: &str) -> usize {
    frame(app, ctx, size, vec![]);
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        },
        |ui| app.render(ui),
    );
    let rect = app
        .hits
        .iter()
        .find(|(name, _)| name == waveform)
        .unwrap()
        .1;
    output
        .shapes
        .iter()
        .filter(|shape| {
            matches!(&shape.shape, Shape::Path(path)
            if path.points.len() == 3 && path.points.iter().all(|&point| rect.contains(point))
            && path.fill != Color32::TRANSPARENT)
        })
        .count()
}

#[test]
fn hot_cue_flags_show_in_library_search_inspect_and_all_player_waveforms() {
    for size in [vec2(800.0, 480.0), vec2(1280.0, 800.0)] {
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
        for style in Style::ALL {
            for overview in [Overview::Half, Overview::Full] {
                app.waveform_style = style;
                app.waveform_overview = overview;
                app.page = Page::Collection;
                app.keyboard = None;
                assert_eq!(flags(&mut app, &ctx, size, "Preview 1"), 4);
                app.keyboard = Some(Field::Search);
                assert_eq!(flags(&mut app, &ctx, size, "Preview 1"), 4);
                app.keyboard = None;
                app.source(Some(app.playlists[0].clone()));
                assert_eq!(flags(&mut app, &ctx, size, "Preview 1"), 4);
                app.inspect(app.tracks[0].clone());
                assert_eq!(flags(&mut app, &ctx, size, "Full waveform"), 4);
                assert!(flags(&mut app, &ctx, size, "Detail waveform") >= 1);
                app.page = Page::Players;
                app.status.players[0].is_playing = false;
                app.status.players[0].play_state = 6;
                app.status.players[0].play_state_name = "CUED".into();
                app.status.players[0].beat_in_track = 0;
                app.deck_compare = true;
                app.deck_focus = None;
                assert_eq!(flags(&mut app, &ctx, size, "Compare overview 1"), 4);
                assert!(flags(&mut app, &ctx, size, "Compare waveform 1") >= 1);
                app.deck_compare = false;
                assert_eq!(flags(&mut app, &ctx, size, "Deck waveform 1"), 4);
                app.deck_focus = Some(1);
                assert_eq!(flags(&mut app, &ctx, size, "Player overview waveform"), 4);
                assert!(flags(&mut app, &ctx, size, "Player detail waveform") >= 1);
            }
        }
    }
}
