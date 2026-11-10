// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::frame;

pub(super) fn click(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    // Allow a newly opened modal/view to complete egui's layout pass.
    for _ in 0..3 {
        frame(app, ctx, size, vec![]);
    }
    crate::app::tests::click(app, ctx, size, label);
}

pub(super) fn app(ctx: &Context) -> App {
    let mut app = App::new(
        ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    app.preview = Controller::test().0;
    app
}

#[test]
fn quick_edits_and_tag_assignment_fit_and_persist_without_starting_preview() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        let ctx = Context::default();
        let mut app = app(&ctx);
        app.inspect(app.all_tracks[0].clone());
        click(&mut app, &ctx, size, "Edit track");
        click(&mut app, &ctx, size, "Edit 5 stars");
        click(&mut app, &ctx, size, "Edit Red");
        click(&mut app, &ctx, size, "Edit Save track");
        assert_eq!(app.detail_track.as_ref().unwrap().rating, 5);
        assert_eq!(app.all_tracks[0].color_id, 2);
        click(&mut app, &ctx, size, "Edit Track tags");
        click(&mut app, &ctx, size, "Track tag 1");
        click(&mut app, &ctx, size, "Edit Save track");
        assert!(app.all_tracks[0].tags.is_empty());
        click(&mut app, &ctx, size, "Track tag 1");
        click(&mut app, &ctx, size, "Edit Cancel edits");
        assert!(
            app.editor
                .as_ref()
                .unwrap()
                .track
                .as_ref()
                .unwrap()
                .tags
                .is_empty()
        );
        click(&mut app, &ctx, size, "Edit Close");
        click(&mut app, &ctx, size, "Edit track");
        assert_eq!(
            app.editor.as_ref().unwrap().track.as_ref().unwrap().rating,
            5
        );
        assert!(!app.preview.request.playing);
    }
}

#[test]
fn category_and_tag_management_create_rename_move_and_confirm_deletion() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.open_tag_manager();
    click(&mut app, &ctx, size, "Edit Categories");
    click(&mut app, &ctx, size, "Edit New category");
    let View::CategoryForm { name, .. } = &mut app.editor.as_mut().unwrap().view else {
        panic!()
    };
    *name = "Energy".into();
    click(&mut app, &ctx, size, "Edit Save category");
    assert_eq!(app.editor.as_ref().unwrap().catalog.categories.len(), 2);
    click(&mut app, &ctx, size, "Edit ‹ Tags");
    click(&mut app, &ctx, size, "Edit New tag");
    let View::TagForm { name, .. } = &mut app.editor.as_mut().unwrap().view else {
        panic!()
    };
    *name = "Peak".into();
    click(&mut app, &ctx, size, "Tag form category 2");
    click(&mut app, &ctx, size, "Edit Save tag");
    click(&mut app, &ctx, size, "Manage tag 2");
    let View::TagForm { name, .. } = &mut app.editor.as_mut().unwrap().view else {
        panic!()
    };
    *name = "Peak time".into();
    click(&mut app, &ctx, size, "Edit Save tag");
    click(&mut app, &ctx, size, "Edit Categories");
    click(&mut app, &ctx, size, "Manage category 2");
    click(&mut app, &ctx, size, "Edit Delete category…");
    assert_eq!(app.editor.as_ref().unwrap().catalog.categories.len(), 2);
    click(&mut app, &ctx, size, "Edit Delete permanently");
    let catalog = &app.editor.as_ref().unwrap().catalog;
    assert_eq!(catalog.categories.len(), 1);
    assert_eq!(
        catalog
            .tags
            .iter()
            .find(|t| t.name == "Peak time")
            .unwrap()
            .category_id,
        0
    );
    click(&mut app, &ctx, size, "Edit ‹ Tags");
    click(&mut app, &ctx, size, "Manage tag 2");
    click(&mut app, &ctx, size, "Edit Delete tag…");
    click(&mut app, &ctx, size, "Edit Cancel deletion");
    assert_eq!(app.editor.as_ref().unwrap().catalog.tags.len(), 2);
    click(&mut app, &ctx, size, "Edit Delete tag…");
    click(&mut app, &ctx, size, "Edit Delete permanently");
    assert_eq!(app.editor.as_ref().unwrap().catalog.tags.len(), 1);
}

#[test]
fn playlist_management_creates_adds_reorders_removes_and_deletes() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.open_playlist_manager(Some(app.all_tracks[0].clone()));
    click(&mut app, &ctx, size, "Edit New playlist");
    let View::PlaylistForm { name, .. } = &mut app.editor.as_mut().unwrap().view else {
        panic!()
    };
    *name = "Tonight".into();
    click(&mut app, &ctx, size, "Edit Save playlist");
    // The new row follows the four demo playlists; filter it into view.
    app.editor.as_mut().unwrap().query = "Tonight".into();
    click(&mut app, &ctx, size, "Edit Tonight");
    let id = app
        .playlists
        .iter()
        .find(|p| p.name == "Tonight")
        .unwrap()
        .id;
    assert_eq!(
        app.playlists.iter().find(|p| p.id == id).unwrap().track_ids,
        [1]
    );
    app.open_playlist_manager(None);
    let p = app.playlists[0].clone();
    app.editor.as_mut().unwrap().view = View::Members {
        before: p.clone(),
        ids: p.track_ids.clone(),
        selected: Some(0),
    };
    click(&mut app, &ctx, size, "Edit ↓");
    click(&mut app, &ctx, size, "Edit Remove");
    click(&mut app, &ctx, size, "Edit Save order");
    assert_eq!(app.playlists[0].track_ids, p.track_ids[1..]);
    app.editor.as_mut().unwrap().query = "Tonight".into();
    click(&mut app, &ctx, size, "Edit Tonight");
    click(&mut app, &ctx, size, "Edit Delete…");
    click(&mut app, &ctx, size, "Edit Delete permanently");
    assert!(!app.playlists.iter().any(|p| p.id == id));
}

#[test]
fn matching_load_and_inspect_use_the_match_selection_outside_the_current_playlist() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.source(Some(app.playlists[0].clone()));
    app.page = Page::Players;
    click(&mut app, &ctx, size, "Match a player");
    click(&mut app, &ctx, size, "Match keys");
    frame(&mut app, &ctx, size, vec![]);
    app.matching.selected = Some(2);
    assert!(!app.tracks.iter().any(|t| t.id == 2));
    assert_eq!(app.selected_track().unwrap().id, 2);
    click(&mut app, &ctx, size, "Match load");
    click(&mut app, &ctx, size, "Load 1");
    assert!(app.load_confirmation.is_some());
    assert_eq!(app.status.players[0].track_id, 1);
    click(&mut app, &ctx, size, "Load cancel");
    app.load_dialog = false;
    click(&mut app, &ctx, size, "Match inspect");
    assert_eq!(app.inspect_origin, Some(Page::Match));
    app.leave_inspect();
    assert_eq!(app.page, Page::Match);
    assert_eq!(app.matching.selected, Some(2));
}

#[test]
fn failed_saves_keep_drafts_require_reload_and_ignore_old_replies() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.open_track_edits(app.all_tracks[0].clone());
    let manager = app.editor.as_mut().unwrap();
    manager.track.as_mut().unwrap().rating = 5;
    let token = manager.token;
    app.edit_response(token.wrapping_sub(1), Err("Old request".into()));
    assert!(app.editor.as_ref().unwrap().error.is_empty());
    app.edit_response(token, Err("Save unconfirmed".into()));
    assert!(app.editor.as_ref().unwrap().reload);
    assert_eq!(
        app.editor.as_ref().unwrap().track.as_ref().unwrap().rating,
        5
    );
    app.connect(&ctx);
    assert!(app.editor.is_none());
}
