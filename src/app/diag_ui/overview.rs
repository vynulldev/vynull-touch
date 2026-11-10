// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[derive(Default)]
pub(in crate::app) struct Requests {
    failures: u64,
    last_error: Option<(Instant, String)>,
    last_status: Option<Instant>,
}

impl Requests {
    pub(in crate::app) fn observe(&mut self, event: &Event) {
        let error = match event {
            Event::Status(at, Ok(_)) => {
                self.last_status = Some(*at);
                None
            }
            Event::Library(Err(error))
            | Event::Edit(_, Err(error))
            | Event::Playlist(_, _, Err(error))
            | Event::Detail(_, _, Err(error))
            | Event::Status(_, Err(error))
            | Event::Load(Err(error))
            | Event::Cues(_, _, _, Err(error))
            | Event::Refresh(_, Err(error)) => Some(error),
            _ => None,
        };
        if let Some(error) = error {
            self.failures = self.failures.saturating_add(1);
            self.last_error = Some((Instant::now(), error.clone()));
        }
    }
}

struct Section {
    title: &'static str,
    lines: Vec<String>,
}

fn age(at: Option<Instant>) -> String {
    at.map_or_else(
        || "Not yet received".into(),
        |at| {
            let seconds = at.elapsed().as_secs_f64();
            if seconds < 60.0 {
                format!("{seconds:.1}s ago")
            } else {
                format!("{seconds:.0}s ago")
            }
        },
    )
}

impl App {
    fn diagnostic_sections(&self) -> [Section; 4] {
        let connection = if self.options.demo {
            "Demo · simulated players".into()
        } else if self.fresh() {
            "Connected".into()
        } else {
            match self.connection {
                api::Connection::Connecting { attempt } => {
                    format!("Connecting · attempt {attempt}")
                }
                api::Connection::Waiting { attempt, until } => format!(
                    "Retrying · attempt {attempt} · {}s",
                    until.saturating_duration_since(Instant::now()).as_secs()
                ),
                api::Connection::Connected => "Stale · waiting for player status".into(),
            }
        };
        let refresh = if self.options.demo {
            "Demo · no background requests"
        } else if !self.library_ready {
            "Waiting for library"
        } else if !self.fresh() {
            "Waiting for connection"
        } else if self.refresh.pending.is_some() {
            "Waiting for current interaction"
        } else if self.refresh.inflight {
            "Checking for changes"
        } else if !self.refresh.error.is_empty() {
            "Retry scheduled"
        } else if self.refresh.force {
            "Manual check queued"
        } else if self.library_session.is_none() || self.refresh.tracks_revision.is_none() {
            "Manual refresh only"
        } else if self.refresh.skip_cues {
            "Automatic · cues need manual Refresh"
        } else {
            "Automatic · checks every 5s"
        };
        let last_status = self.last_status.or(self.diagnostics.requests.last_status);
        let mut versions = vec![
            format!("UI: {}", crate::build_info::VERSION),
            format!("Build: {}", crate::build_info::ID),
            crate::build_info::TARGET.into(),
            format!(
                "Vynull: {}",
                self.diagnostics.version.description(self.options.demo)
            ),
        ];
        if !self.options.demo {
            versions.push(format!(
                "Version checked: {}",
                age(self.diagnostics.version.updated)
            ));
        }
        let mut errors = vec![format!(
            "API: {} · Logs: {} · Version: {}",
            self.diagnostics.requests.failures,
            self.diagnostics.logs.failures,
            self.diagnostics.version.failures
        )];
        if let Some((at, error)) = &self.diagnostics.requests.last_error {
            errors.push(format!("Last API error ({}): {error}", age(Some(*at))));
        }
        for (label, error) in [
            ("Connection", &self.connection_error),
            ("Refresh", &self.refresh.error),
            ("Version", &self.diagnostics.version.error),
            ("Logs", &self.diagnostics.logs.error),
        ] {
            if !error.is_empty() {
                errors.push(format!("{label}: {error}"));
            }
        }
        if errors.len() == 1 {
            errors.push("No current errors".into());
        }
        [
            Section {
                title: "VERSIONS",
                lines: versions,
            },
            Section {
                title: "CONNECTION",
                lines: vec![
                    connection,
                    if self.options.demo {
                        "Demo server".into()
                    } else {
                        self.options.server.clone()
                    },
                    format!(
                        "Player update: {}",
                        if self.options.demo {
                            "Simulated".into()
                        } else {
                            age(last_status)
                        }
                    ),
                    format!(
                        "Players: {} · Link: {}",
                        self.status.players.len(),
                        if !self.fresh() {
                            "Unknown"
                        } else if self.linked {
                            "Active"
                        } else {
                            "Inactive"
                        }
                    ),
                ],
            },
            Section {
                title: "LIBRARY",
                lines: vec![
                    if self.library_ready {
                        format!("Loaded tracks: {}", self.all_tracks.len())
                    } else {
                        "Tracks: not loaded".into()
                    },
                    format!("Refresh: {refresh}"),
                    format!(
                        "Last library check: {}",
                        if self.options.demo {
                            "Demo".into()
                        } else {
                            age(self.refresh.checked)
                        }
                    ),
                    format!(
                        "Track revision: {}",
                        self.refresh
                            .tracks_revision
                            .map_or_else(|| "Not reported".into(), |rev| rev.to_string())
                    ),
                ],
            },
            Section {
                title: "REQUEST ERRORS",
                lines: errors,
            },
        ]
    }

    pub(super) fn diagnostic_overview(&self, ui: &mut Ui) {
        let sections = self.diagnostic_sections();
        ScrollArea::vertical().id_salt("diagnostic-overview")
            .scroll_source(scroll_area::ScrollSource::ALL)
            .auto_shrink([false, false]).show(ui, |ui| {
                ui.columns(2, |columns| {
                    for (index, section) in sections.iter().enumerate() {
                        Frame::new().fill(PANEL).corner_radius(8).inner_margin(12)
                            .show(&mut columns[index % 2], |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(RichText::new(section.title).size(11.0).color(ORANGE));
                                for line in &section.lines {
                                    ui.add(Label::new(RichText::new(line).size(13.0)).wrap());
                                }
                            });
                    }
                });
                ui.label(RichText::new("Error counts cover API, log and version requests since connecting. Artwork, waveform and audio fetches are separate.").size(11.0).color(MUTED));
            });
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }

    pub(super) fn diagnostic_report(&self) -> String {
        let mut text = String::from("Vynull Touch diagnostic report\n\n");
        for section in self.diagnostic_sections() {
            text.push_str(section.title);
            text.push('\n');
            for line in section.lines {
                text.push_str(&line);
                text.push('\n');
            }
            text.push('\n');
        }
        if let Some(info) = &self.diagnostics.version.info {
            text.push_str(&format!(
                "Daemon commit: {}\nDaemon build date: {}\n",
                info.commit, info.date
            ));
        }
        text.push_str("Error counts cover API, log and version requests since connecting; artwork, waveform and audio fetches are separate.\n");
        text.push_str(&format!(
            "\nSERVER LOG SNAPSHOT · {} entries · {} · fetched {}\n",
            self.diagnostics.logs.entries.len(),
            if self.diagnostics.logs.paused {
                "paused"
            } else {
                "not paused"
            },
            age(self.diagnostics.logs.updated)
        ));
        text.push_str(&self.diagnostics.logs.copy_text());
        text
    }
}
