// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn browser_reordering_skips_hidden_entries_and_required_categories_stay_shown() {
    let mut draft = Snapshot::demo();
    draft.validate().unwrap();
    assert_eq!(
        (
            draft.items.len(),
            draft.sorts.len(),
            draft.track_detail_fields.len()
        ),
        (22, 17, 16)
    );
    draft.toggle_category("track");
    assert!(
        draft
            .items
            .iter()
            .find(|v| v.key == "track")
            .unwrap()
            .visible
    );
    draft.toggle_category("album");
    draft.move_category("track", -1);
    assert_eq!(draft.items[0].key, "track");
    assert_eq!(draft.items[1].key, "album");
    assert!(!draft.items[1].visible);
    draft.move_category("track", -1);
    assert_eq!(draft.items[0].key, "track");
    assert!(
        draft
            .items
            .iter()
            .find(|v| v.key == "folder")
            .unwrap()
            .export_only
    );
    draft.move_sort(1, -1);
    assert_eq!(draft.sorts[0].id, 1);
    draft.toggle_sort(1);
    draft.move_sort(2, -1);
    assert_eq!(draft.sorts[0].id, 1, "hidden sort stays in place");
    assert_eq!(draft.sorts[1].id, 2, "moves across visible sorts");
    for item in draft.sorts.clone() {
        if item.visible {
            draft.toggle_sort(item.id);
        }
    }
    assert_eq!(draft.sorts.iter().filter(|v| v.visible).count(), 1);
    draft.validate().unwrap();
}

#[test]
fn browser_changes_send_only_edited_components_and_keep_fresh_labels() {
    let before = Snapshot::demo();
    let mut draft = before.clone();
    draft.move_category("album", -1);
    draft.toggle_category("artist");
    let edit = Edit {
        before: before.clone(),
        draft,
    };
    let mut latest = before;
    latest.track_detail = "key".into();
    latest.move_sort(2, -1);
    latest.items[0].label = "CUSTOM ARTIST".into();
    let (body, target) = edit.prepare(&latest).unwrap();
    assert_eq!(
        body.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["items"]
    );
    assert_eq!(body["items"][1]["label"], "CUSTOM ARTIST");
    assert_eq!(body["items"][1]["visible"], false);
    assert_eq!(body["items"][0]["key"], "album");
    assert_eq!(target.track_detail, "key");
    assert!(target.same_sorts(&latest));
    edit.verify(&target, &target).unwrap();
    let mut readback = target.clone();
    readback.track_detail = "time".into();
    edit.verify(&target, &readback).unwrap();
    readback.items.swap(0, 1);
    assert!(edit.verify(&target, &readback).is_err());
}

#[test]
fn browser_preflight_rejects_overlapping_changes_and_invalid_drafts() {
    for part in 0..3 {
        let before = Snapshot::demo();
        let mut draft = before.clone();
        let mut latest = before.clone();
        match part {
            0 => {
                draft.move_category("album", -1);
                latest.toggle_category("key");
            }
            1 => {
                draft.move_sort(1, -1);
                latest.toggle_sort(3);
            }
            _ => {
                draft.track_detail = "artist".into();
                latest.track_detail = "key".into();
            }
        }
        let edit = Edit {
            before: before.clone(),
            draft,
        };
        assert!(
            edit.prepare(&latest)
                .unwrap_err()
                .contains("changed on Vynull")
        );
    }
    let before = Snapshot::demo();
    for invalid in 0..4 {
        let mut draft = before.clone();
        match invalid {
            0 => draft.items[2].visible = false,
            1 => draft.items.push(draft.items[0].clone()),
            2 => draft.sorts.iter_mut().for_each(|s| s.visible = false),
            _ => draft.track_detail = "invalid".into(),
        }
        assert!(
            Edit {
                before: before.clone(),
                draft
            }
            .prepare(&before)
            .is_err()
        );
    }
}

#[test]
fn browser_detail_only_does_not_send_menus_and_uses_fresh_capabilities() {
    let before = Snapshot::demo();
    let mut draft = before.clone();
    draft.track_detail = "not_specified".into();
    let edit = Edit {
        before: before.clone(),
        draft,
    };
    assert_eq!(
        edit.prepare(&before).unwrap().0,
        json!({"track_detail":"not_specified"})
    );
    let mut changed = before.clone();
    changed
        .track_detail_fields
        .retain(|f| f.key != "not_specified");
    assert!(
        edit.prepare(&changed)
            .unwrap_err()
            .contains("no longer available")
    );
    assert_eq!(
        Edit {
            before: before.clone(),
            draft: before.clone()
        }
        .prepare(&before)
        .unwrap()
        .0,
        json!({})
    );
}
