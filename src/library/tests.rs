// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn ordering_is_stable_natural_and_keeps_unknown_values_last_in_both_directions() {
    let tracks = vec![
        Track {
            title: "Beta".into(),
            bpm: 120.0,
            key: "10A".into(),
            ..Default::default()
        },
        Track {
            title: "alpha".into(),
            bpm: 128.0,
            key: "2A".into(),
            ..Default::default()
        },
        Track {
            title: "ALPHA".into(),
            bpm: 128.0,
            key: "2B".into(),
            ..Default::default()
        },
        Track::default(),
        Track {
            bpm: f64::NAN,
            ..Default::default()
        },
    ];
    for (field, descending, expected) in [
        (Sort::Title, false, vec![1, 2, 0, 3, 4]),
        (Sort::Title, true, vec![0, 1, 2, 3, 4]),
        (Sort::Bpm, false, vec![0, 1, 2, 3, 4]),
        (Sort::Bpm, true, vec![1, 2, 0, 3, 4]),
        (Sort::Key, false, vec![1, 2, 0, 3, 4]),
        (Sort::Key, true, vec![0, 2, 1, 3, 4]),
        (Sort::Original, true, vec![0, 1, 2, 3, 4]),
    ] {
        let mut visible: Vec<_> = (0..tracks.len()).collect();
        Order { field, descending }.apply(&tracks, &mut visible);
        assert_eq!(visible, expected, "{field:?}, descending={descending}");
    }
    let tracks: Vec<_> = ["z", "a", "b"]
        .into_iter()
        .enumerate()
        .map(|(i, value)| Track {
            artist: value.into(),
            album: value.into(),
            duration: [300, 100, 200][i],
            rating: [5, 0, 3][i],
            date_added: format!("2026-01-{:02}", [30, 1, 20][i]),
            ..Default::default()
        })
        .collect();
    for field in [
        Sort::Artist,
        Sort::Album,
        Sort::Duration,
        Sort::Rating,
        Sort::Added,
    ] {
        let mut visible = vec![0, 1, 2];
        Order {
            field,
            descending: false,
        }
        .apply(&tracks, &mut visible);
        assert_eq!(visible, [1, 2, 0]);
        Order {
            field,
            descending: true,
        }
        .apply(&tracks, &mut visible);
        assert_eq!(visible, [0, 2, 1]);
    }
}

#[test]
fn filters_combine_categories_accept_inclusive_ranges_and_handle_unset_metadata() {
    let track: Track = serde_json::from_str(r#"{"id":1,"bpm":128.5,"key":"8A","genre":" Deep House ","rating":4,"color_id":3,"date_added":"2026-11-01","tags":["Warm Up","Any"]}"#).unwrap();
    let mut filters = Filters {
        min_bpm: Some(128.5),
        max_bpm: Some(128.5),
        keys: vec!["8a".into(), "9b".into()],
        genres: vec!["deep house".into()],
        rating: Some(4),
        colors: vec![1, 3],
        tags: vec!["warm up".into()],
    };
    assert!(filters.matches(&track));
    filters.max_bpm = Some(128.4);
    assert!(!filters.matches(&track));
    filters.max_bpm = None;
    filters.rating = Some(5);
    assert!(!filters.matches(&track));
    filters.rating = Some(0);
    assert!(!filters.matches(&track));
    filters.rating = None;
    filters.tags = vec!["any".into()];
    assert!(filters.matches(&track));
    filters.tags = vec!["missing".into()];
    assert!(!filters.matches(&track));
    let blank: Track = serde_json::from_str(r#"{"id":2,"tags":null}"#).unwrap();
    assert_eq!(blank.rating, 0);
    assert_eq!(blank.color_id, 0);
    assert!(Filters::default().matches(&blank));
    assert!(
        !Filters {
            min_bpm: Some(1.0),
            ..Default::default()
        }
        .matches(&blank)
    );
    assert!(
        Filters {
            keys: vec!["".into()],
            genres: vec!["".into()],
            tags: vec!["".into()],
            rating: Some(0),
            colors: vec![0],
            ..Default::default()
        }
        .matches(&blank)
    );
    for (min, max) in [
        ("NaN", ""),
        ("-1", ""),
        ("0", ""),
        ("", "infinity"),
        ("130", "120"),
        ("1000", ""),
        ("abc", ""),
        ("1.2.3", ""),
    ] {
        assert!(bpm_range(min, max).is_err(), "accepted {min}–{max}");
    }
    assert_eq!(bpm_range("", "128.5"), Ok((None, Some(128.5))));
    assert_eq!(bpm_range("120", ""), Ok((Some(120.0), None)));
}
