// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn compatible_keys_accept_standard_enharmonic_and_camelot_with_wheel_wrap() {
    for (a, b) in [
        ("Am", "8A"),
        ("Abm", "1a"),
        ("B♭m", "03A"),
        ("Db", "3B"),
        ("F#", "Gb"),
    ] {
        assert_eq!(relation(a, b), Some((0, "Same key")));
    }
    assert_eq!(relation("Am", "C"), Some((1, "Relative key")));
    assert_eq!(relation("1A", "12A"), Some((1, "Adjacent key")));
    assert_eq!(relation("Am", "F#"), None);
    assert_eq!(relation("", "8A"), None);
    assert_eq!(relation("unknown", "unknown"), None);
}

#[test]
fn matches_use_pitched_tempo_and_external_ids_do_not_alias_library_tracks() {
    let mut state = State::default();
    state.select_player(Player {
        track_id: 1,
        bpm: 120.0,
        pitch_pct: 10.0,
        key: "Am".into(),
        ..Default::default()
    });
    let tracks = vec![
        Track {
            id: 1,
            bpm: 132.0,
            key: "8A".into(),
            ..Default::default()
        },
        Track {
            id: 2,
            bpm: 132.0,
            key: "C".into(),
            ..Default::default()
        },
        Track {
            id: 3,
            bpm: 66.0,
            key: "Am".into(),
            ..Default::default()
        },
        Track {
            id: 4,
            bpm: 132.0,
            key: "F#".into(),
            ..Default::default()
        },
        Track {
            id: 5,
            bpm: 132.0,
            key: "Am".into(),
            file_missing: true,
            ..Default::default()
        },
    ];
    state.update(&tracks);
    assert_eq!(state.results.iter().map(|m| m.id).collect::<Vec<_>>(), [2]);
    state.half_double = true;
    state.dirty = true;
    state.update(&tracks);
    assert_eq!(
        state.results.iter().map(|m| m.id).collect::<Vec<_>>(),
        [3, 2]
    );
    assert!(state.results[0].reason.contains("double time"));
    state.seed.as_mut().unwrap().external = true;
    state.dirty = true;
    state.update(&tracks);
    assert!(state.results.iter().any(|m| m.id == 1));
    state.keys = false;
    state.dirty = true;
    state.update(&tracks);
    assert!(state.results.iter().any(|m| m.id == 4));
    state.selected = Some(2);
    state.seed.as_mut().unwrap().bpm = 0.0;
    state.dirty = true;
    state.update(&tracks);
    assert!(state.results.is_empty());
}
