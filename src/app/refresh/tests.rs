// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};
use eframe::egui;

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
    app
}

fn snapshot(app: &App) -> Snapshot {
    Snapshot {
        tracks_revision: Some(2),
        tracks: None,
        playlists: app.playlists.clone(),
        playlist_tracks: None,
        cues_revision: None,
        cues: None,
        skip_cues: false,
        detail: None,
    }
}

fn queue(app: &mut App, snapshot: Snapshot) {
    app.refresh.pending = Some((
        Request {
            generation: app.refresh.generation,
            token: app.token,
            source: app.source,
            detail: app.detail_track.as_ref().map(|t| t.id),
            ..Default::default()
        },
        snapshot,
    ));
}

#[test]
fn library_preview_cues_follow_batch_reads_verified_edits_deletes_and_identity_changes() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    let mut cue = demo::cues()[0].clone();
    let mut update = snapshot(&app);
    update.cues = Some(HashMap::from([(1, vec![cue.clone()])]));
    update.cues_revision = Some(1);
    queue(&mut app, update);
    app.refresh_tick(&ctx);
    assert_eq!(app.refresh.cues(1), [cue.clone()]);

    let (worker, _commands, replies) = Worker::test();
    app.worker = Some(worker);
    app.options.demo = false;
    app.inspect(app.tracks[0].clone());
    cue.time_ms = 4000;
    replies
        .send(Event::Detail(
            1,
            app.token,
            Ok(Detail {
                cues: vec![cue.clone()],
                cues_ready: true,
                ..Default::default()
            }),
        ))
        .unwrap();
    app.poll(&ctx);
    assert_eq!(app.refresh.cues(1), [cue.clone()]);
    cue.time_ms = 8000;
    cue.color_id = 0x2a;
    replies
        .send(Event::Cues(
            1,
            app.token,
            true,
            Ok(crate::cues::Saved {
                cues: vec![cue.clone()],
                persisted: true,
            }),
        ))
        .unwrap();
    app.poll(&ctx);
    assert_eq!(app.refresh.cues(1), [cue.clone()]);
    assert_eq!(app.refresh.cues_revision, Some(1));
    replies
        .send(Event::Cues(
            1,
            app.token - 1,
            true,
            Ok(crate::cues::Saved {
                cues: vec![],
                persisted: true,
            }),
        ))
        .unwrap();
    app.poll(&ctx);
    assert_eq!(
        app.refresh.cues(1),
        [cue.clone()],
        "ignore an old request's result"
    );
    replies
        .send(Event::Cues(
            1,
            app.token,
            true,
            Ok(crate::cues::Saved {
                cues: vec![],
                persisted: true,
            }),
        ))
        .unwrap();
    app.poll(&ctx);
    assert!(app.refresh.cues(1).is_empty());
    app.refresh.remember_cues(1, &[cue.clone()]);
    let mut update = snapshot(&app);
    update.cues = Some(HashMap::new());
    queue(&mut app, update);
    app.refresh_tick(&ctx);
    assert!(
        app.refresh.cues(1).is_empty(),
        "batch deletion removes markers"
    );
    app.refresh.remember_cues(1, &[cue.clone()]);
    let mut update = snapshot(&app);
    let mut tracks = app.all_tracks.clone();
    tracks[0].file_path = "replacement.flac".into();
    update.tracks = Some(tracks);
    queue(&mut app, update);
    app.refresh_tick(&ctx);
    assert!(
        app.refresh.cues(1).is_empty(),
        "a reused ID must not inherit cues"
    );
    app.refresh.remember_cues(1, &[cue]);
    app.options.demo = true;
    app.connect(&ctx);
    assert!(app.refresh.cues(1).is_empty());
}

#[test]
fn playlist_totals_follow_membership_not_filters_and_clear_on_reconnect() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.playlists[0].is_smart = true;
    app.playlists[0].track_ids.clear();
    let playlist = app.playlists[0].clone();
    let (worker, _requests, events) = Worker::test();
    app.worker = Some(worker);
    app.options.demo = false;
    assert_eq!(
        app.playlist_subtitle(&playlist),
        "Smart playlist · Counting…"
    );
    app.source(Some(playlist.clone()));
    events
        .send(Event::Playlist(
            playlist.id,
            app.token,
            Ok(app.all_tracks[..3].to_vec()),
        ))
        .unwrap();
    app.poll(&ctx);
    app.query = "no matching tracks".into();
    app.filter();
    assert!(app.visible.is_empty());
    assert_eq!(
        app.playlist_subtitle(&playlist),
        "Smart playlist · 3 tracks"
    );

    for count in [1, 0] {
        let mut update = snapshot(&app);
        update.playlist_tracks = Some(app.all_tracks[..count].to_vec());
        queue(&mut app, update);
        app.refresh_tick(&ctx);
        assert_eq!(
            app.playlist_subtitle(&playlist),
            if count == 1 {
                "Smart playlist · 1 track"
            } else {
                "Smart playlist · 0 tracks"
            }
        );
    }
    assert_eq!(app.playlist_subtitle(&app.playlists[1]), "24 tracks");
    assert_eq!(
        app.playlist_subtitle(&Playlist {
            is_folder: true,
            ..Default::default()
        }),
        "Folder"
    );
    app.options.demo = true;
    app.connect(&ctx);
    assert_eq!(app.playlist_counts.count(playlist.id), None);
}

#[test]
fn refreshed_collection_keeps_top_track_selection_filters_and_search() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        let ctx = Context::default();
        let mut app = app(&ctx);
        app.query = "a".into();
        app.filters.rating = Some(1);
        app.order = Order {
            field: crate::library::Sort::Title,
            descending: true,
        };
        app.reindex();
        frame(&mut app, &ctx, size, vec![]);
        let scroll = app.library_scroll.unwrap();
        track_scroll::set_offset(&ctx, scroll, 107.0);
        frame(&mut app, &ctx, size, vec![]);
        let top = app.tracks[app.visible[2]].id;
        app.selected = Some(top);
        let mut update = snapshot(&app);
        let mut tracks = app.all_tracks.clone();
        let first = app.tracks[app.visible[0]].id;
        tracks.retain(|t| t.id != first);
        update.tracks = Some(tracks);
        queue(&mut app, update);
        frame(&mut app, &ctx, size, vec![]);
        assert_eq!(app.selected, Some(top));
        assert_eq!(app.query, "a");
        assert_eq!(app.filters.rating, Some(1));
        assert!(app.order.descending);
        assert_eq!(app.tracks[app.visible[1]].id, top);
        assert!((track_scroll::offset(&ctx, scroll) - 57.0).abs() < 0.1);
    }
}

#[test]
fn automatic_refresh_is_bounded_backs_off_and_keeps_manual_fallback() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    let (mut worker, _commands, events) = Worker::test();
    let (refresh, requests) = std::sync::mpsc::sync_channel(1);
    worker.refresh = refresh;
    app.worker = Some(worker);
    app.options.demo = false;
    app.library_session = Some("one".into());
    app.last_status = Some(Instant::now());
    app.refresh.next = Instant::now();
    app.refresh_tick(&ctx);
    assert!(
        requests.try_recv().is_err(),
        "old servers need manual Refresh"
    );
    app.refresh.tracks_revision = Some(1);
    app.refresh_tick(&ctx);
    let request = requests.try_recv().unwrap();
    for _ in 0..5 {
        app.refresh_tick(&ctx);
    }
    assert!(requests.try_recv().is_err(), "only one outstanding read");
    assert!(app.refresh.inflight);
    events
        .send(Event::Refresh(request, Err("temporary outage".into())))
        .unwrap();
    app.poll(&ctx);
    assert!(!app.refresh.inflight);
    assert_eq!(app.refresh.tracks_revision, Some(1));
    assert!(app.refresh.next > Instant::now());
    assert!(!app.refresh.error.is_empty());
    assert!(requests.try_recv().is_err());
    app.refresh.next = Instant::now();
    app.refresh_tick(&ctx);
    let request = requests.try_recv().unwrap();
    events
        .send(Event::Refresh(request, Ok(Box::new(snapshot(&app)))))
        .unwrap();
    app.poll(&ctx);
    assert_eq!(app.refresh.tracks_revision, Some(2));
    assert!(app.refresh.error.is_empty());
}

#[test]
fn navigating_away_discards_old_playlist_snapshot() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    let mut update = snapshot(&app);
    update.tracks = Some(vec![]);
    queue(&mut app, update);
    let playlist = app.playlists.iter().find(|p| !p.is_folder).unwrap().clone();
    app.source(Some(playlist.clone()));
    let tracks = app.tracks.clone();
    app.refresh_tick(&ctx);
    assert_eq!(app.source, Some(playlist.id));
    assert_eq!(app.tracks, tracks);
    assert_eq!(app.refresh.tracks_revision, None);
}

#[test]
fn session_change_from_refresh_clears_old_drafts_even_while_editing() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.selected = Some(1);
    app.inspect(app.all_tracks[0].clone());
    app.open_cue_editor();
    let (worker, _, events) = Worker::test();
    app.worker = Some(worker);
    events
        .send(Event::Refresh(
            Request::default(),
            Err("Library changed; reconnect".into()),
        ))
        .unwrap();
    app.poll(&ctx);
    assert!(app.cue_editor.is_none());
    assert!(app.detail_track.is_none());
    assert!(app.selected.is_none());
    assert!(app.notice.contains("Library session changed"));
}

#[test]
fn refresh_waits_for_inertial_scrolling_to_settle() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    let size = vec2(800.0, 480.0);
    frame(&mut app, &ctx, size, vec![]);
    let start = app
        .hits
        .iter()
        .find(|(name, _)| name == "Track 4")
        .unwrap()
        .1
        .center();
    frame(
        &mut app,
        &ctx,
        size,
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    for i in 1..=4 {
        frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::PointerMoved(
                start - vec2(0.0, i as f32 * 20.0),
            )],
        );
    }
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerButton {
            pos: start - vec2(0.0, 80.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(track_scroll::moving(&ctx, app.library_scroll.unwrap()));
    let update = snapshot(&app);
    queue(&mut app, update);
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.refresh.pending.is_some());
    for _ in 0..300 {
        frame(&mut app, &ctx, size, vec![]);
        if app.refresh.pending.is_none() {
            break;
        }
    }
    assert!(app.refresh.pending.is_none());
}

#[test]
fn refresh_waits_for_touch_release_so_a_tap_selects_the_drawn_track() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    let size = vec2(800.0, 480.0);
    frame(&mut app, &ctx, size, vec![]);
    let target = app.tracks[app.visible[1]].id;
    let rect = app
        .hits
        .iter()
        .find(|(name, _)| name == &format!("Track {target}"))
        .unwrap()
        .1;
    let mut update = snapshot(&app);
    let mut tracks = app.all_tracks.clone();
    tracks.remove(0);
    update.tracks = Some(tracks);
    queue(&mut app, update);
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            size,
            vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(app.refresh.pending.is_some());
    }
    assert_eq!(app.selected, Some(target));
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.refresh.pending.is_none());
    assert_eq!(app.selected, Some(target));
}

#[test]
fn cue_draft_and_playhead_survive_until_editor_closes_then_cues_refresh() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.inspect(app.all_tracks[0].clone());
    app.position = 32.0;
    app.open_cue_editor();
    app.cue_editor.as_mut().unwrap().draft.label = "Unsaved label".into();
    let mut update = snapshot(&app);
    update.cues = Some(HashMap::from([(1, vec![])]));
    update.cues_revision = Some(9);
    let old_cues = app.detail.as_ref().unwrap().cues.clone();
    queue(&mut app, update);
    app.refresh_tick(&ctx);
    assert_eq!(app.detail.as_ref().unwrap().cues, old_cues);
    assert_eq!(
        app.cue_editor.as_ref().unwrap().draft.label,
        "Unsaved label"
    );
    assert_eq!(app.refresh.cues_revision, None);
    app.cue_editor = None;
    app.refresh_tick(&ctx);
    assert!(app.detail.as_ref().unwrap().cues.is_empty());
    assert_eq!(app.refresh.cues_revision, Some(9));
    assert_eq!(app.position, 32.0);
    assert_eq!(app.page, Page::Detail);
}

#[test]
fn local_cue_command_discards_older_snapshot_without_consuming_its_revision() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    app.inspect(app.all_tracks[0].clone());
    let (worker, commands, _events) = Worker::test();
    app.worker = Some(worker);
    let mut update = snapshot(&app);
    update.cues = Some(HashMap::new());
    update.cues_revision = Some(5);
    queue(&mut app, update);
    let old_cues = app.detail.as_ref().unwrap().cues.clone();
    assert!(app.send(Command::Cues {
        track: Box::new(app.detail_track.clone().unwrap()),
        token: app.token,
        expected: None,
        change: None
    }));
    assert!(matches!(commands.try_recv().unwrap(), Command::Cues { .. }));
    app.refresh_tick(&ctx);
    assert!(app.refresh.pending.is_none());
    assert_eq!(app.refresh.cues_revision, None);
    assert_eq!(app.detail.as_ref().unwrap().cues, old_cues);
}

#[test]
fn playlist_membership_rename_and_deletion_preserve_context_where_possible() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    let playlist = app.playlists.iter().find(|p| !p.is_folder).unwrap().clone();
    app.source(Some(playlist.clone()));
    app.query = "a".into();
    app.reindex();
    let mut update = snapshot(&app);
    update
        .playlists
        .iter_mut()
        .find(|p| p.id == playlist.id)
        .unwrap()
        .name = "Renamed set".into();
    let mut tracks = app.tracks.clone();
    tracks.reverse();
    update.playlist_tracks = Some(tracks.clone());
    queue(&mut app, update);
    app.refresh_tick(&ctx);
    assert_eq!(app.source, Some(playlist.id));
    assert_eq!(app.source_name, "Renamed set");
    assert_eq!(app.tracks, tracks);
    assert_eq!(app.query, "a");
    let mut update = snapshot(&app);
    update.playlists.clear();
    queue(&mut app, update);
    app.refresh_tick(&ctx);
    assert_eq!(app.source, None);
    assert_eq!(app.query, "a");
    assert!(app.notice.contains("playlist was removed"));
}

#[test]
fn removed_or_reused_track_stops_preview_and_clears_inspect_and_selection() {
    for removed in [true, false] {
        let ctx = Context::default();
        let mut app = app(&ctx);
        app.selected = Some(1);
        app.inspect(app.all_tracks[0].clone());
        let mut update = snapshot(&app);
        let mut tracks = app.all_tracks.clone();
        if removed {
            tracks.remove(0);
        } else {
            tracks[0].file_path = "/different.flac".into();
        }
        update.tracks = Some(tracks);
        queue(&mut app, update);
        app.refresh_tick(&ctx);
        assert_eq!(app.selected, None);
        assert!(app.detail_track.is_none());
        assert!(app.preview.request.track.is_none());
        assert_eq!(app.page, Page::Collection);
    }
}

#[test]
fn setup_refresh_keeps_playlist_and_search_and_requests_forced_background_read() {
    let ctx = Context::default();
    let mut app = app(&ctx);
    let playlist = app.playlists.iter().find(|p| !p.is_folder).unwrap().clone();
    app.source(Some(playlist));
    let source = app.source;
    app.query = "a".into();
    app.page = Page::Settings;
    app.options.demo = false;
    // No worker needed: verify the request remains pending without clearing UI state.
    click(&mut app, &ctx, vec2(800.0, 480.0), "Library refresh");
    assert!(app.refresh.force);
    assert_eq!(app.page, Page::Settings);
    assert_eq!(app.source, source);
    assert_eq!(app.query, "a");
}
