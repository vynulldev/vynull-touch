// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[cfg(test)]
mod tests;

impl App {
    fn set_theme(&mut self, ctx: &Context, theme: theme::UiTheme) {
        self.ui_theme = theme;
        theme::apply(ctx, theme);
        self.notice = format!("{} theme selected", theme.label());
        if let Some(path) = &self.settings_path
            && let Err(error) = settings::save_theme(path, theme)
        {
            self.notice = format!("Theme changed, but could not save it: {error}");
        }
    }

    pub(super) fn theme_modal(&mut self, ctx: &Context) {
        let mut close = false;
        let modal = Modal::new(Id::unique("theme-picker")).show(ctx, |ui| {
            ui.set_width(480.0_f32.min(ctx.content_rect().width() - 48.0));
            ui.heading("Interface theme");
            ui.label("Switch instantly. Your choice is saved for next time.");
            ui.add_space(8.0);
            for theme in theme::UiTheme::ALL {
                let choice = theme::button(
                    ui,
                    theme.label(),
                    vec2(ui.available_width(), 56.0),
                    self.ui_theme == theme,
                );
                self.hit(&format!("Theme {}", theme.label()), choice.rect);
                if choice.clicked() {
                    self.set_theme(ctx, theme);
                }
                ui.label(
                    RichText::new(match theme {
                        theme::UiTheme::Classic => {
                            "Warm orange accents, rounded controls and the original player layout."
                        }
                        theme::UiTheme::XdjAz => {
                            "Blue accents, large deck readouts and player controls in the top bar."
                        }
                    })
                    .size(14.0),
                );
                ui.add_space(8.0);
            }
            let done = theme::primary(ui, "Done", vec2(112.0, 48.0));
            self.hit("Theme done", done.rect);
            close = done.clicked();
        });
        self.theme_picker = !close && !modal.should_close();
    }
}
