// SPDX-License-Identifier: GPL-3.0-or-later
mod api;
mod app;
mod artwork;
mod beat_grid;
mod build_info;
mod cues;
mod decks;
mod demo;
mod diagnostics;
mod editing;
mod library;
mod loops;
mod matching;
mod model;
mod phrases;
mod preview;
mod settings;
mod theme;
mod waveform;

use eframe::egui;

#[derive(Clone, Debug)]
pub struct Options {
    demo: bool,
    server: String,
    server_given: bool,
    startup_page: Option<settings::StartupPage>,
    fullscreen: bool,
    size: [f32; 2],
    scale: f32,
    screenshot: Option<String>,
    smoke_test: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            demo: false,
            server: "http://127.0.0.1:9443".into(),
            server_given: false,
            startup_page: None,
            fullscreen: false,
            size: [800.0, 480.0],
            scale: 1.0,
            screenshot: None,
            smoke_test: false,
        }
    }
}

fn options(args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--demo" => options.demo = true,
            "--server" => {
                options.server = args.next().ok_or("--server needs an HTTP(S) origin")?;
                api::validate_origin(&options.server)?;
                options.server_given = true;
            }
            "--startup-page" => {
                options.startup_page = Some(match args.next().as_deref() {
                    Some("players") => settings::StartupPage::Players,
                    Some("collection") => settings::StartupPage::Collection,
                    Some("playlists") => settings::StartupPage::Playlists,
                    _ => return Err("--startup-page needs players, collection or playlists".into()),
                });
            }
            "--fullscreen" => options.fullscreen = true,
            "--size" => {
                let size = args.next().ok_or("--size needs WIDTHxHEIGHT")?;
                let (w, h) = size.split_once('x').ok_or("Use --size 800x480")?;
                options.size = [
                    w.parse().map_err(|_| "Invalid width")?,
                    h.parse().map_err(|_| "Invalid height")?,
                ];
                if !options
                    .size
                    .iter()
                    .all(|v| v.is_finite() && (480.0..=4096.0).contains(v))
                    || options.size[0] < 800.0
                {
                    return Err("Minimum layout is 800x480; maximum dimension is 4096".into());
                }
            }
            "--scale" => {
                options.scale = args
                    .next()
                    .ok_or("--scale needs a number")?
                    .parse()
                    .map_err(|_| "Invalid scale")?;
                if !options.scale.is_finite() || !(0.5..=3.0).contains(&options.scale) {
                    return Err("Scale must be between 0.5 and 3".into());
                }
            }
            "--screenshot" => {
                options.screenshot = Some(args.next().ok_or("--screenshot needs a PNG path")?)
            }
            "--smoke-test" => options.smoke_test = true,
            _ => return Err(format!("Unknown option: {arg}")),
        }
    }
    if options.demo && options.server_given {
        return Err("Choose --demo or --server".into());
    }
    Ok(Some(options))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "--version" | "-V") {
        println!("{}", build_info::description());
        return Ok(());
    }
    let Some(options) = options(args.into_iter()).map_err(std::io::Error::other)? else {
        println!(
            "Vynull Touch — native touch companion\n\n  --demo              Fictional collection; no network or audio\n  --server URL        Override saved server (default http://127.0.0.1:9443)\n  --startup-page PAGE Override saved tab: players, collection, playlists\n  --fullscreen        Fill the display\n  --size WIDTHxHEIGHT Window size (default 800x480)\n  --scale NUMBER      UI zoom (default 1)\n  --screenshot FILE   Capture a PNG, then exit\n  --smoke-test        Render a few frames, then exit\n  --version           Show UI version, build ID and target\n  --help              Show help"
        );
        return Ok(());
    };
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vynull Touch")
            .with_inner_size(options.size)
            .with_min_inner_size([800.0, 480.0])
            .with_fullscreen(options.fullscreen),
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        ..Default::default()
    };
    eframe::run_native(
        "vynull-touch",
        native,
        Box::new(move |cc| {
            Ok(Box::new(app::App::new(
                &cc.egui_ctx,
                options,
                settings::path(),
            )))
        }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_options_before_starting_network_or_ui() {
        for args in [
            vec!["--wat"],
            vec!["--server"],
            vec!["--demo", "--server", "http://localhost"],
            vec!["--scale", "NaN"],
            vec!["--size", "800x0"],
            vec!["--server", "http://user:pass@host"],
            vec!["--startup-page"],
            vec!["--startup-page", "inspect"],
        ] {
            assert!(options(args.into_iter().map(str::to_owned)).is_err());
        }
    }

    #[test]
    fn command_line_preferences_are_explicit_and_startup_accepts_only_supported_tabs() {
        let defaults = options(std::iter::empty()).unwrap().unwrap();
        assert!(!defaults.server_given);
        assert!(defaults.startup_page.is_none());
        for (name, expected) in [
            ("players", settings::StartupPage::Players),
            ("collection", settings::StartupPage::Collection),
            ("playlists", settings::StartupPage::Playlists),
        ] {
            let parsed = options(
                ["--server", "http://127.0.0.1:9443", "--startup-page", name]
                    .into_iter()
                    .map(str::to_owned),
            )
            .unwrap()
            .unwrap();
            assert!(parsed.server_given);
            assert_eq!(parsed.startup_page, Some(expected));
        }
    }
}
