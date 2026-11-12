// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl App {
    pub(in crate::app) fn stop_library_preview(&mut self) {
        if self.library_preview.take().is_some() {
            self.preview.stop();
        }
    }

    pub(in crate::app) fn sync_library_preview(&mut self) {
        let Some(track) = &self.library_preview else {
            return;
        };
        let current = self.page == Page::Collection
            && self.keyboard != Some(Field::Search)
            && self.preview.request.origin == self.options.server
            && self.preview.request.session == self.library_session
            && self
                .preview
                .request
                .track
                .is_some_and(|(id, _)| id == track.id)
            && self.selected_track().is_some_and(|selected| {
                selected.id == track.id
                    && selected.file_path == track.file_path
                    && !selected.file_missing
            });
        if !current {
            self.stop_library_preview();
        }
    }

    pub(in crate::app) fn library_preview_button(&mut self, ui: &mut Ui, track: &Track) {
        self.sync_library_preview();
        let playing = self.library_preview.is_some()
            && self.preview.request.playing
            && !matches!(self.preview.state.phase, Phase::Error | Phase::Ended);
        let enabled = !track.file_missing && (self.options.demo || self.library_session.is_some());
        let button = ui
            .add_enabled_ui(enabled, |ui| {
                theme::button(
                    ui,
                    if playing { "Pause" } else { "Preview" },
                    vec2(96.0, 48.0),
                    playing,
                )
            })
            .inner
            .on_hover_text("Preview on the audio output selected in Settings");
        self.hit("Library preview", button.rect);
        if button.clicked() {
            if self.library_preview.is_none() {
                self.preview.load(
                    (track.id, track.duration),
                    self.options.server.clone(),
                    self.library_session.clone(),
                    self.options.demo,
                );
                self.library_preview = Some(track.clone());
                self.position = 0.0;
                // A previous Inspect snapshot no longer owns this audio session.
                self.detail_track = None;
                self.detail = None;
                self.detail_busy = false;
            }
            self.preview.toggle(self.position);
            if self.preview.state.phase == Phase::Ended {
                self.position = 0.0;
            }
        }
    }

    pub(in crate::app) fn library_preview_status(&self) -> Option<String> {
        let track = self.library_preview.as_ref()?;
        let state = &self.preview.state;
        let progress = format!(
            "{} / {}",
            time(self.position),
            time(self.preview_duration(track)),
        );
        let status = match state.phase {
            Phase::Loading => "Preparing audio… Tap Pause to cancel".into(),
            Phase::Buffering => format!("Buffering audio… · {progress}"),
            Phase::Playing => format!(
                "{progress} · {}",
                state.output_name.as_deref().unwrap_or("System default")
            ),
            Phase::Ended => "Finished · tap Preview to restart".into(),
            Phase::Error => state
                .error
                .clone()
                .unwrap_or_else(|| "Audio unavailable".into()),
            _ if self.preview.request.playing => "Preparing audio… Tap Pause to cancel".into(),
            _ => format!("Paused · {progress}"),
        };
        Some(format!("Preview · {} · {status}", track.title))
    }
}
