// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::frame;
use eframe::egui::Event as InputEvent;

fn setup() -> (App, Context) {
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
    (app, ctx)
}

fn touch(app: &mut App, ctx: &Context, size: Vec2, pos: Pos2, phase: TouchPhase) {
    let mut events = vec![
        InputEvent::PointerMoved(pos),
        InputEvent::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        },
    ];
    if phase != TouchPhase::Move {
        events.push(InputEvent::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: phase == TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    if phase == TouchPhase::End {
        events.push(InputEvent::PointerGone);
    }
    frame(app, ctx, size, events);
}

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    for _ in 0..3 {
        frame(app, ctx, size, vec![]);
    }
    let rect = app
        .hits
        .iter()
        .find(|(name, _)| name == label)
        .unwrap_or_else(|| panic!("Missing {label}"))
        .1;
    assert!(
        Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
        "{label}: {rect:?}"
    );
    touch(app, ctx, size, rect.center(), TouchPhase::Start);
    touch(app, ctx, size, rect.center(), TouchPhase::End);
}

#[test]
fn prepare_touch_create_add_reorder_remove_inspect_and_load_preserve_browsing() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let (mut app, ctx) = setup();
        app.source(Some(app.playlists[0].clone()));
        app.selected = Some(app.tracks[0].id);
        let track = app.selected_track().unwrap().clone();
        let original_source = app.source;
        let original_order = app.order;
        tap(&mut app, &ctx, size, "Prepare add");
        assert!(app.prepare.picker);
        tap(&mut app, &ctx, size, "Prepare create");
        assert!(!app.prepare.picker);
        assert_eq!(app.prepare_playlist().unwrap().track_ids, [track.id]);
        tap(&mut app, &ctx, size, "Prepare add");
        assert_eq!(app.prepare_playlist().unwrap().track_ids.len(), 1);
        frame(&mut app, &ctx, size, vec![]);
        assert!(
            app.hits
                .iter()
                .any(|(name, _)| *name == format!("Prepared track {}", track.id))
        );
        let second = app
            .all_tracks
            .iter()
            .find(|t| t.id != track.id)
            .unwrap()
            .clone();
        app.inspect(second.clone());
        tap(&mut app, &ctx, size, "Prepare add");
        assert_eq!(
            app.prepare_playlist().unwrap().track_ids,
            [track.id, second.id]
        );
        let preview_revision = app.preview.request.revision;
        tap(&mut app, &ctx, size, "Prepare add");
        assert_eq!(app.prepare_playlist().unwrap().track_ids, [track.id]);
        assert!(!app.prepared(&second));
        assert!(app.all_tracks.iter().any(|t| t.id == second.id));
        assert_eq!(app.page, Page::Detail);
        assert_eq!(app.preview.request.revision, preview_revision);
        tap(&mut app, &ctx, size, "Prepare add");
        assert_eq!(
            app.prepare_playlist().unwrap().track_ids,
            [track.id, second.id]
        );
        tap(&mut app, &ctx, size, "Back");
        assert_eq!(app.page, Page::Collection);
        assert_eq!(app.source, original_source);
        assert_eq!(app.order, original_order);
        tap(&mut app, &ctx, size, "Prepare");
        tap(&mut app, &ctx, size, "Prepare track 1");
        tap(&mut app, &ctx, size, "Prepare Up");
        assert_eq!(
            app.prepare_playlist().unwrap().track_ids,
            [second.id, track.id]
        );
        assert_eq!(app.selected_track().unwrap().id, second.id);
        tap(&mut app, &ctx, size, "Prepare inspect");
        assert_eq!(app.detail_track.as_ref().unwrap().id, second.id);
        assert_eq!(app.navigation_page(), Page::Prepare);
        tap(&mut app, &ctx, size, "Back");
        assert_eq!(app.page, Page::Prepare);
        tap(&mut app, &ctx, size, "Prepare load");
        assert!(app.load_dialog);
        for player in &mut app.status.players {
            player.is_playing = false;
            player.play_state = 0;
        }
        tap(&mut app, &ctx, size, "Load 2");
        assert_eq!(
            app.status
                .players
                .iter()
                .find(|p| p.device_number == 2)
                .unwrap()
                .track_id,
            second.id
        );
        assert_eq!(app.prepare_playlist().unwrap().track_ids.len(), 2);
        tap(&mut app, &ctx, size, "Prepare remove");
        assert_eq!(app.prepare_playlist().unwrap().track_ids, [track.id]);
        assert!(app.all_tracks.iter().any(|t| t.id == second.id));
        assert!(!app.preview.request.playing);
        tap(&mut app, &ctx, size, "Collection");
        assert_eq!(app.source, original_source);
        assert_eq!(app.selected, Some(track.id));
    }
}

#[test]
fn selecting_existing_playlist_and_adding_match_keeps_members_and_ignores_read_only_lists() {
    let size = vec2(800.0, 480.0);
    let (mut app, ctx) = setup();
    for (id, folder, smart, read_only) in [
        (90, true, false, false),
        (91, false, true, false),
        (92, false, false, true),
        (1 << 30, false, false, false),
    ] {
        app.playlists.push(Playlist {
            id,
            name: "Unavailable".into(),
            is_folder: folder,
            is_smart: smart,
            read_only,
            ..Default::default()
        });
    }
    app.page = Page::Prepare;
    tap(&mut app, &ctx, size, "Prepare choose");
    frame(&mut app, &ctx, size, vec![]);
    for id in [90, 91, 92, 1 << 30] {
        assert!(
            !app.hits
                .iter()
                .any(|(name, _)| *name == format!("Prepare playlist {id}"))
        );
    }
    let original = app.playlists[0].clone();
    tap(
        &mut app,
        &ctx,
        size,
        &format!("Prepare playlist {}", original.id),
    );
    assert_eq!(app.prepare_playlist().unwrap(), &original);
    app.open_matches();
    app.matching.keys = false;
    app.matching.tolerance = 10.0;
    app.matching.dirty = true;
    app.matching.update(&app.all_tracks);
    let id = app
        .matching
        .results
        .iter()
        .find(|m| !original.track_ids.contains(&m.id))
        .unwrap()
        .id;
    app.matching.selected = Some(id);
    tap(&mut app, &ctx, size, "Prepare add");
    let mut expected = original.track_ids;
    expected.push(id);
    assert_eq!(app.prepare_playlist().unwrap().track_ids, expected);
    assert_eq!(app.page, Page::Match);
}

#[test]
fn binding_persists_for_same_library_and_demo_never_overwrites_real_choice() {
    let (mut app, ctx) = setup();
    let root = std::env::temp_dir().join(format!("vynull-prepare-settings-{}", std::process::id()));
    let path = root.join("settings.json");
    settings::save_hot_cue_colors(&path, [49; 8]).unwrap();
    app.settings_path = Some(path.clone());
    app.options.demo = false;
    app.library_session = Some("library-one".into());
    app.bind_prepare(app.playlists[0].id);
    let binding = app.prepare.binding.clone().unwrap();
    let mut restored = App::new(
        &Context::default(),
        Options {
            demo: true,
            ..Default::default()
        },
        Some(path.clone()),
    );
    assert_eq!(restored.prepare.binding, Some(binding.clone()));
    restored.options.demo = false;
    restored.library_session = Some("library-one".into());
    assert!(restored.prepare_playlist().is_some());
    restored.library_session = Some("library-two".into());
    assert!(restored.prepare_playlist().is_none());
    restored.library_session = Some("library-one".into());
    restored.options.server = "http://another-library:9443".into();
    assert!(restored.prepare_playlist().is_none());
    app.options.demo = true;
    app.bind_prepare(app.playlists[1].id);
    settings::save_zoom(&path, 32.0).unwrap();
    let saved = settings::load(Some(&path));
    assert_eq!(saved.prepare_playlist, Some(binding));
    assert_eq!(saved.hot_cue_colors, [49; 8]);
    app.prepare.adding = Some(app.all_tracks[0].clone());
    app.prepare.pending = Some(Pending {
        token: 123,
        action: Action::Add,
    });
    app.connect(&ctx);
    assert!(app.prepare.pending.is_none());
    assert!(app.prepare.adding.is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_or_stale_write_reply_requires_reload_without_replaying_add() {
    let size = vec2(800.0, 480.0);
    let (mut app, ctx) = setup();
    app.options.demo = false;
    app.library_session = Some("one".into());
    app.last_status = Some(Instant::now());
    app.bind_prepare(app.playlists[0].id);
    let before = app.prepare_playlist().unwrap().clone();
    let track = app
        .all_tracks
        .iter()
        .find(|t| !before.track_ids.contains(&t.id))
        .unwrap()
        .clone();
    let (worker, requests, events) = Worker::test();
    app.worker = Some(worker);
    app.add_to_prepare(track.clone());
    let Command::Edit(token, request) = requests.try_recv().unwrap() else {
        panic!();
    };
    let Request::Members {
        before: expected,
        ids,
        added,
    } = *request
    else {
        panic!();
    };
    assert_eq!(expected, before);
    assert_eq!(ids.last(), Some(&track.id));
    assert_eq!(added[0].file_path, track.file_path);
    app.add_to_prepare(track.clone());
    assert!(requests.try_recv().is_err());
    events
        .send(Event::Edit(
            token.wrapping_add(1),
            Ok(Reply::Playlists(vec![])),
        ))
        .unwrap();
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.prepare.pending.is_some());
    events
        .send(Event::Edit(
            token,
            Err("Save was not confirmed; reload".into()),
        ))
        .unwrap();
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.prepare.reload);
    app.add_to_prepare(track.clone());
    assert!(requests.try_recv().is_err());
    // The write actually applied before its reply was lost. Reload discovers it.
    let mut after = app.playlists.clone();
    after
        .iter_mut()
        .find(|p| p.id == before.id)
        .unwrap()
        .track_ids
        .push(track.id);
    app.prepare_submit(Request::Playlists, Action::Read);
    let Command::Edit(token, request) = requests.try_recv().unwrap() else {
        panic!();
    };
    assert!(matches!(*request, Request::Playlists));
    events
        .send(Event::Edit(token, Ok(Reply::Playlists(after))))
        .unwrap();
    frame(&mut app, &ctx, size, vec![]);
    assert!(!app.prepare.reload);
    assert!(app.prepared(&track));
    app.add_to_prepare(track.clone());
    assert!(requests.try_recv().is_err());

    app.inspect(track.clone());
    assert!(matches!(
        requests.try_recv().unwrap(),
        Command::Detail(_, _)
    ));
    tap(&mut app, &ctx, size, "Prepare add");
    let Command::Edit(token, request) = requests.try_recv().unwrap() else {
        panic!();
    };
    let Request::Members { ids, added, .. } = *request else {
        panic!();
    };
    assert_eq!(ids, before.track_ids);
    assert!(added.is_empty());
    assert!(
        app.prepared(&track),
        "Keep the saved state until removal is verified"
    );
    tap(&mut app, &ctx, size, "Prepare add");
    assert!(
        requests.try_recv().is_err(),
        "A second tap must not queue a write while saving"
    );
    events
        .send(Event::Edit(
            token,
            Err("Removal was not confirmed; reload".into()),
        ))
        .unwrap();
    frame(&mut app, &ctx, size, vec![]);
    tap(&mut app, &ctx, size, "Prepare add");
    assert!(requests.try_recv().is_err());
    assert!(app.prepared(&track));
    assert!(app.prepare.reload);
}

#[test]
fn prepare_touch_scroll_keeps_taps_stable_and_missing_tracks_can_only_be_removed() {
    let size = vec2(800.0, 480.0);
    let (mut app, ctx) = setup();
    app.playlists[0].track_ids = app.all_tracks.iter().map(|t| t.id).collect();
    app.bind_prepare(app.playlists[0].id);
    app.page = Page::Prepare;
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![]);
    }
    let list = app
        .hits
        .iter()
        .find(|(name, _)| name == "Prepare tracks")
        .unwrap()
        .1;
    let start = list.center_bottom() - vec2(0.0, 20.0);
    touch(&mut app, &ctx, size, start, TouchPhase::Start);
    touch(
        &mut app,
        &ctx,
        size,
        start - vec2(0.0, 90.0),
        TouchPhase::Move,
    );
    touch(
        &mut app,
        &ctx,
        size,
        start - vec2(0.0, 90.0),
        TouchPhase::End,
    );
    assert!(app.prepare.selected.is_none());
    track_scroll::set_offset(&ctx, app.prepare_scroll_id(), 0.0);
    tap(&mut app, &ctx, size, "Prepare track 0");
    assert_eq!(track_scroll::offset(&ctx, app.prepare_scroll_id()), 0.0);
    assert_eq!(app.prepare.selected.as_ref().unwrap().index, 0);
    let selected_id = app.prepare.selected.as_ref().unwrap().id;
    app.all_tracks
        .iter_mut()
        .find(|t| t.id == selected_id)
        .unwrap()
        .file_path = "different-file".into();
    assert!(app.selected_track().is_none());
    app.playlists[0].track_ids = vec![u32::MAX];
    tap(&mut app, &ctx, size, "Prepare track 0");
    tap(&mut app, &ctx, size, "Prepare load");
    tap(&mut app, &ctx, size, "Prepare inspect");
    assert!(!app.load_dialog);
    assert_eq!(app.page, Page::Prepare);
    tap(&mut app, &ctx, size, "Prepare remove");
    assert!(app.prepare_playlist().unwrap().track_ids.is_empty());
}

#[test]
fn shared_playlist_refresh_updates_membership_and_defers_to_foreground_writes() {
    use crate::api::refresh::{Request as RefreshRequest, Snapshot};
    let (mut app, ctx) = setup();
    app.bind_prepare(app.playlists[0].id);
    app.source(Some(app.playlists[1].clone()));
    app.page = Page::Prepare;
    let original = app.prepare_playlist().unwrap().clone();
    let added = app
        .all_tracks
        .iter()
        .find(|t| !original.track_ids.contains(&t.id))
        .unwrap()
        .clone();
    let mut updated = app.playlists.clone();
    updated
        .iter_mut()
        .find(|p| p.id == original.id)
        .unwrap()
        .track_ids = vec![added.id];
    let queue = |app: &mut App, playlists: Vec<Playlist>, generation: u64| {
        app.refresh.pending = Some((
            RefreshRequest {
                generation,
                token: app.token,
                source: app.source,
                ..Default::default()
            },
            Snapshot {
                tracks_revision: None,
                tracks: None,
                playlists,
                playlist_tracks: None,
                cues_revision: None,
                cues: None,
                skip_cues: false,
                detail: None,
            },
        ));
    };
    let generation = app.refresh.generation;
    queue(&mut app, updated.clone(), generation);
    app.prepare.pending = Some(Pending {
        token: 42,
        action: Action::Read,
    });
    app.refresh_tick(&ctx);
    assert_eq!(app.prepare_playlist().unwrap(), &original);
    app.prepare.pending = None;
    app.refresh_tick(&ctx);
    assert_eq!(app.prepare_playlist().unwrap().track_ids, [added.id]);
    assert!(app.prepared(&added));

    // A delayed refresh from before a verified add cannot undo the new state.
    let track = app
        .all_tracks
        .iter()
        .find(|t| t.id != added.id)
        .unwrap()
        .clone();
    app.add_to_prepare(track.clone());
    queue(&mut app, updated, generation);
    app.refresh_tick(&ctx);
    assert_eq!(
        app.prepare_playlist().unwrap().track_ids,
        [added.id, track.id]
    );

    // Reload can discover that the unrelated Collection source was deleted.
    let mut after = app.playlists.clone();
    after.retain(|p| Some(p.id) != app.source);
    app.prepare.pending = Some(Pending {
        token: 43,
        action: Action::Read,
    });
    app.prepare_response(Ok(Reply::Playlists(after)));
    assert!(app.source.is_none());
    assert_eq!(app.page, Page::Prepare);
    assert_eq!(
        app.prepare_playlist().unwrap().track_ids,
        [added.id, track.id]
    );
}
