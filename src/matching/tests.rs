// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn live_reference_requires_a_unique_connected_loaded_deck() {
    use live::Reference;
    let mut status = crate::demo::status();
    assert_eq!(
        Reference::Master
            .player(&status, true)
            .unwrap()
            .device_number,
        1
    );
    assert!(Reference::Master.player(&status, false).is_none());
    assert!(Reference::Off.player(&status, true).is_none());
    assert!(Reference::Player(2).player(&status, true).is_none());
    status.players[1] = Player {
        device_number: 2,
        is_master: false,
        ..status.players[0].clone()
    };
    assert_eq!(
        Reference::Player(2)
            .player(&status, true)
            .unwrap()
            .device_number,
        2
    );
    status.players[1].is_master = true;
    assert!(Reference::Master.player(&status, true).is_none());
    status.players[0].is_master = false;
    assert_eq!(
        Reference::Master
            .player(&status, true)
            .unwrap()
            .device_number,
        2
    );
    status.players[1].is_playing = false;
    status.players[1].play_state = 6;
    status.players[1].play_state_name = "CUED".into();
    assert!(Reference::Master.player(&status, true).is_some());
    status.players[1].play_state_name = "LOADING".into();
    assert!(Reference::Master.player(&status, true).is_none());
    status.players[1].play_state_name = "PAUSED".into();
    status.players.push(Player {
        is_master: false,
        ..status.players[1].clone()
    });
    assert!(Reference::Master.player(&status, true).is_none());
    status.players.pop();
    status.peers.pop();
    assert!(Reference::Master.player(&status, true).is_none());
    assert!(Reference::Player(2).player(&status, true).is_none());
}

#[test]
fn tempo_ranges_include_boundaries_half_double_time_and_reject_invalid_values() {
    assert!(tempo_match(106.0, 100.0, 6.0, false).is_some());
    assert!(tempo_match(94.0, 100.0, 6.0, false).is_some());
    assert!(tempo_match(106.01, 100.0, 6.0, false).is_none());
    assert!(tempo_match(132.0, 66.0, 6.0, false).is_none());
    assert_eq!(tempo_match(132.0, 66.0, 6.0, true), Some((2.0, 0.0)));
    assert_eq!(tempo_match(66.0, 132.0, 6.0, true), Some((0.5, 0.0)));
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(tempo_match(120.0, invalid, 6.0, true).is_none());
        assert!(tempo_match(invalid, 120.0, 6.0, true).is_none());
    }
    assert!(tempo_match(120.0, 120.0, f64::NAN, false).is_none());
    assert!(
        tempo(&Player {
            bpm: -120.0,
            pitch_pct: -200.0,
            ..Default::default()
        })
        .is_none()
    );
    assert!(
        tempo(&Player {
            bpm: 120.0,
            pitch_pct: f64::NAN,
            ..Default::default()
        })
        .is_none()
    );
}

#[test]
fn compatible_keys_accept_standard_enharmonic_and_camelot_with_wheel_wrap() {
    let rules = KeyRules::default();
    for (a, b) in [
        ("Am", "8A"),
        ("Abm", "1a"),
        ("B♭m", "03A"),
        ("Db", "3B"),
        ("F#", "Gb"),
    ] {
        assert_eq!(rules.relation(a, b), Some((0, "Same key")));
    }
    assert_eq!(rules.relation("Am", "C"), Some((1, "Relative key")));
    assert_eq!(rules.relation("1A", "12A"), Some((1, "Adjacent key")));
    assert_eq!(rules.relation("Am", "F#"), None);
    assert_eq!(rules.relation("", "8A"), None);
    assert_eq!(rules.relation("unknown", "unknown"), None);
}

#[test]
fn key_rules_select_neighbors_independently_in_both_modes_and_wrap_both_directions() {
    for mask in 0..8 {
        let rules = KeyRules {
            relative: mask & 1 != 0,
            next: mask & 2 != 0,
            previous: mask & 4 != 0,
        };
        for number in 1..=12 {
            for mode in ["A", "B"] {
                let reference = format!("{number}{mode}");
                assert_eq!(
                    rules.relation(&reference, &reference),
                    Some((0, "Same key"))
                );
                let other = if mode == "A" { "B" } else { "A" };
                assert_eq!(
                    rules
                        .relation(&reference, &format!("{number}{other}"))
                        .is_some(),
                    rules.relative
                );
                assert_eq!(
                    rules
                        .relation(&reference, &format!("{}{mode}", number % 12 + 1))
                        .is_some(),
                    rules.next
                );
                assert_eq!(
                    rules
                        .relation(&reference, &format!("{}{mode}", (number + 10) % 12 + 1))
                        .is_some(),
                    rules.previous
                );
                // Selecting a direction never includes an adjacent key in the other mode.
                assert!(
                    rules
                        .relation(&reference, &format!("{}{other}", number % 12 + 1))
                        .is_none()
                );
                assert!(
                    rules
                        .relation(&reference, &format!("{}{mode}", (number + 1) % 12 + 1))
                        .is_none()
                );
            }
        }
        assert_eq!(rules.relation("B♭m", "Db").is_some(), rules.relative);
        assert_eq!(rules.relation("B♭m", "Fm").is_some(), rules.next);
        assert_eq!(rules.relation("B♭m", "Ebm").is_some(), rules.previous);
        assert!(rules.relation("", "3A").is_none());
    }
}

#[test]
fn changed_key_rules_filter_results_and_clear_a_now_incompatible_selection() {
    let mut state = State::default();
    state.select_player(Player {
        bpm: 120.0,
        key: "3A".into(),
        ..Default::default()
    });
    let tracks = ["3A", "3B", "4A", "2A", "4B"]
        .into_iter()
        .enumerate()
        .map(|(index, key)| Track {
            id: index as u32 + 1,
            bpm: 120.0,
            key: key.into(),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    state.update(&tracks);
    assert_eq!(
        state.results.iter().map(|m| m.id).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    state.selected = Some(3);
    state.key_rules.next = false;
    state.dirty = true;
    state.update(&tracks);
    assert_eq!(
        state.results.iter().map(|m| m.id).collect::<Vec<_>>(),
        [1, 2, 4]
    );
    assert!(state.selected.is_none());
    state.key_rules = KeyRules {
        relative: false,
        next: false,
        previous: false,
    };
    state.dirty = true;
    state.update(&tracks);
    assert_eq!(state.results.iter().map(|m| m.id).collect::<Vec<_>>(), [1]);
    state.keys = false;
    state.dirty = true;
    state.update(&tracks);
    assert_eq!(
        state.results.len(),
        5,
        "Key unrestricted still includes disabled neighbors"
    );
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
