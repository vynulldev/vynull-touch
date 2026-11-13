// SPDX-License-Identifier: GPL-3.0-or-later
mod format;
mod input;
mod wav;
use eframe::egui::Context;
pub use format::{BitDepth, SampleRate};
pub use input::{Device, devices};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::Ordering, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub input: Option<Device>,
    pub directory: String,
    pub sample_rate: SampleRate,
    pub bit_depth: BitDepth,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            input: None,
            directory: "~/Music/Vynull Recordings".into(),
            sample_rate: SampleRate::default(),
            bit_depth: BitDepth::default(),
        }
    }
}
impl Preferences {
    pub fn directory(&self) -> Result<PathBuf, String> {
        let path = if let Some(suffix) = self.directory.strip_prefix("~/") {
            PathBuf::from(std::env::var_os("HOME").ok_or("Home directory is unavailable")?)
                .join(suffix)
        } else {
            PathBuf::from(&self.directory)
        };
        if !path.is_absolute() {
            return Err("Enter an absolute folder path, or a path starting with ~/".into());
        }
        Ok(path)
    }
}

/// Shows a path under the home folder as `~/…`, as the folder setting writes it.
pub fn display_path(path: &Path) -> String {
    home_relative(path, std::env::var_os("HOME").as_deref().map(Path::new))
}
fn home_relative(path: &Path, home: Option<&Path>) -> String {
    // A home of `/` would turn every path into `~/…`.
    match home
        .filter(|home| home.parent().is_some())
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub active: bool,
    pub recording: bool,
    pub input_name: String,
    pub rate: u32,
    pub bit_depth: BitDepth,
    pub frames: u64,
    pub paths: Vec<PathBuf>,
    pub peak: [f32; 2],
    pub rms: [f32; 2],
    pub clipped: [bool; 2],
    pub error: Option<String>,
    ack: u64,
}
impl State {
    pub fn elapsed(&self) -> String {
        let seconds = self.frames / u64::from(self.rate.max(1));
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    }
}

enum Command {
    Begin(Preferences, bool),
    Record(PathBuf),
    Stop,
    ClearClips,
    Shutdown,
}

pub struct Controller {
    pub state: State,
    shared: Arc<Mutex<State>>,
    worker: Option<JoinHandle<()>>,
    tx: Option<mpsc::Sender<(u64, Command)>>,
    serial: u64,
    ctx: Context,
}
impl Controller {
    pub fn new(ctx: Context) -> Self {
        Self {
            state: State::default(),
            shared: Arc::new(Mutex::new(State::default())),
            worker: None,
            tx: None,
            serial: 0,
            ctx,
        }
    }
    pub fn busy(&self) -> bool {
        self.state.ack != self.serial
    }
    pub fn poll(&mut self) {
        self.state = self.shared.lock().unwrap().clone();
    }
    fn send(&mut self, command: Command) {
        if self.tx.is_none() {
            let (tx, rx) = mpsc::channel();
            let shared = self.shared.clone();
            let ctx = self.ctx.clone();
            self.worker = Some(thread::spawn(move || run(rx, shared, ctx)));
            self.tx = Some(tx);
        }
        self.serial += 1;
        if self
            .tx
            .as_ref()
            .unwrap()
            .send((self.serial, command))
            .is_err()
        {
            self.state.error = Some("The recording worker stopped unexpectedly. Restart the app before recording again.".into());
            self.state.active = false;
            self.state.recording = false;
            self.state.ack = self.serial;
            *self.shared.lock().unwrap() = self.state.clone();
        }
    }
    pub fn begin(&mut self, preferences: Preferences, record: bool) {
        self.send(Command::Begin(preferences, record));
    }
    pub fn record(&mut self, directory: PathBuf) {
        self.send(Command::Record(directory));
    }
    pub fn stop(&mut self) {
        self.send(Command::Stop);
    }
    pub fn clear_clips(&mut self) {
        self.send(Command::ClearClips);
    }
    #[cfg(test)]
    pub fn set_test_state(&mut self, state: State) {
        *self.shared.lock().unwrap() = state.clone();
        self.state = state;
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send((self.serial + 1, Command::Shutdown));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Engine {
    input: Option<input::Input>,
    session: Option<wav::Session>,
    state: State,
    buffer: Vec<[f32; 2]>,
    checkpoint: Instant,
    last_audio: Instant,
}
impl Engine {
    fn new() -> Self {
        Self {
            input: None,
            session: None,
            state: State::default(),
            buffer: Vec::with_capacity(4096),
            checkpoint: Instant::now(),
            last_audio: Instant::now(),
        }
    }
    fn begin(&mut self, preferences: Preferences, record: bool) -> Result<(), String> {
        if self.input.is_some() {
            return Err("Stop the current input before changing it.".into());
        }
        self.state = State {
            ack: self.state.ack,
            ..Default::default()
        };
        let input = input::open(preferences.input.as_ref(), preferences.sample_rate)?;
        self.state = State {
            active: true,
            input_name: input.name.clone(),
            rate: input.rate,
            bit_depth: preferences.bit_depth,
            ack: self.state.ack,
            ..Default::default()
        };
        self.input = Some(input);
        self.last_audio = Instant::now();
        if record {
            self.record(preferences.directory()?)?;
        }
        Ok(())
    }
    fn record(&mut self, directory: PathBuf) -> Result<(), String> {
        if self.session.is_some() {
            return Err("Already recording".into());
        }
        let input = self.input.as_mut().ok_or("Open an input first")?;
        // Metering before Record is not part of the saved session.
        let pending = input.frames.slots();
        for _ in 0..pending {
            let _ = input.frames.pop();
        }
        let session = wav::Session::create(&directory, input.rate, self.state.bit_depth)
            .map_err(|e| format!("Cannot create recording: {e}"))?;
        self.state.frames = 0;
        self.state.paths = session.paths.clone();
        self.state.error = None;
        self.state.recording = true;
        self.session = Some(session);
        self.checkpoint = Instant::now();
        Ok(())
    }
    fn drain(&mut self) -> Result<(), String> {
        let Some(input) = &mut self.input else {
            return Ok(());
        };
        let mut peak = [0.0f32; 2];
        let mut sums = [0.0f64; 2];
        let count = input.frames.slots();
        for _ in 0..count {
            let Ok(frame) = input.frames.pop() else {
                break;
            };
            for c in 0..2 {
                let sample = if frame[c].is_finite() {
                    frame[c].abs()
                } else {
                    0.0
                };
                peak[c] = peak[c].max(sample);
                sums[c] += f64::from(sample).powi(2);
                self.state.clipped[c] |= sample >= 0.9999;
            }
            self.buffer.push(frame);
            if self.buffer.len() == 4096 {
                let result = if let Some(session) = &mut self.session {
                    session
                        .write(&self.buffer)
                        .map_err(|e| format!("Recording write failed: {e}"))
                } else {
                    Ok(())
                };
                self.buffer.clear();
                result?;
            }
        }
        if let Some(session) = &mut self.session {
            let result = session
                .write(&self.buffer)
                .map_err(|e| format!("Recording write failed: {e}"));
            self.buffer.clear();
            result?;
            self.state.frames = session.frames;
            if self.state.paths.len() != session.paths.len() {
                self.state.paths = session.paths.clone();
            }
            if self.checkpoint.elapsed() >= Duration::from_secs(1) {
                session
                    .checkpoint()
                    .map_err(|e| format!("Recording checkpoint failed: {e}"))?;
                self.checkpoint = Instant::now();
            }
        }
        self.buffer.clear();
        if count > 0 {
            self.last_audio = Instant::now();
        }
        for c in 0..2 {
            self.state.peak[c] = peak[c].max(self.state.peak[c] * 0.92);
            self.state.rms[c] = (sums[c] / count.max(1) as f64).sqrt() as f32;
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<(), String> {
        self.drain()?;
        if let Some(input) = &self.input {
            if input.overflow.swap(false, Ordering::Relaxed) {
                return Err(
                    "Recording stopped: audio buffer overrun. The saved audio may contain a gap."
                        .into(),
                );
            }
            if let Ok(error) = input.errors.try_recv() {
                return Err(error);
            }
            if self.last_audio.elapsed() > Duration::from_secs(5) {
                return Err("Audio input stopped delivering samples.".into());
            }
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<(), String> {
        if let Some(input) = &mut self.input {
            input.stream.take();
        }
        let drain = self.tick();
        self.input = None;
        let finish = if let Some(mut session) = self.session.take() {
            self.state.frames = session.frames;
            self.state.paths = session.paths.clone();
            session
                .checkpoint()
                .map_err(|e| format!("Cannot finalize recording: {e}"))
        } else {
            Ok(())
        };
        self.state.active = false;
        self.state.recording = false;
        self.state.peak = [0.0; 2];
        self.state.rms = [0.0; 2];
        self.buffer.clear();
        drain.and(finish)
    }
}

fn run(rx: mpsc::Receiver<(u64, Command)>, shared: Arc<Mutex<State>>, ctx: Context) {
    let mut engine = Engine::new();
    loop {
        let mut shutdown = false;
        let message = if engine.state.active {
            rx.recv_timeout(Duration::from_millis(20))
        } else {
            rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        let result = match message {
            Ok((serial, command)) => {
                engine.state.ack = serial;
                match command {
                    Command::Begin(preferences, record) => engine.begin(preferences, record),
                    Command::Record(directory) => engine.record(directory),
                    Command::Stop => engine.stop(),
                    Command::ClearClips => {
                        engine.state.clipped = [false; 2];
                        Ok(())
                    }
                    Command::Shutdown => {
                        shutdown = true;
                        engine.stop()
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => engine.tick(),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                shutdown = true;
                engine.stop()
            }
        };
        if let Err(error) = result {
            let finish = engine.stop();
            engine.state.error = Some(match finish {
                Ok(()) => error,
                Err(e) => format!("{error}; {e}"),
            });
        }
        *shared.lock().unwrap() = engine.state.clone();
        if engine.state.active || engine.state.ack > 0 {
            ctx.request_repaint();
        }
        if shutdown {
            break;
        }
    }
}

pub fn db(amplitude: f32) -> f32 {
    20.0 * amplitude.max(0.000_001).log10()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::AtomicBool,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn input() -> (Engine, rtrb::Producer<[f32; 2]>, mpsc::Sender<String>) {
        let (producer, frames) = rtrb::RingBuffer::new(16);
        let (errors_tx, errors) = mpsc::channel();
        let mut engine = Engine::new();
        engine.input = Some(input::Input {
            stream: None,
            frames,
            overflow: Arc::new(AtomicBool::new(false)),
            errors,
            name: "Synthetic stereo".into(),
            rate: 48_000,
        });
        engine.state.active = true;
        engine.state.rate = 48_000;
        (engine, producer, errors_tx)
    }

    #[test]
    fn older_recording_preferences_keep_their_input_and_folder() {
        for format in [
            "",
            r#", "sample_rate":"future_rate", "bit_depth":"future_depth""#,
        ] {
            let json = format!(
                r#"{{"directory":"/tmp/Recordings","input":{{"id":"pulse:input","name":"HiFiBerry","host":"PulseAudio"}}{format}}}"#
            );
            let saved: Preferences = serde_json::from_str(&json).unwrap();
            assert_eq!(saved.directory, "/tmp/Recordings");
            assert_eq!(saved.input.unwrap().name, "HiFiBerry");
            assert_eq!(saved.sample_rate, SampleRate::Auto);
            assert_eq!(saved.bit_depth, BitDepth::Pcm24);
        }
    }

    #[test]
    fn recording_excludes_monitor_audio_and_stop_drains_and_finalizes() {
        let (mut engine, mut producer, _errors) = input();
        producer.push([1.0, 0.25]).unwrap();
        engine.tick().unwrap();
        assert_eq!(engine.state.clipped, [true, false]);
        assert_eq!(engine.state.peak, [1.0, 0.25]);
        assert_eq!(engine.state.rms, [1.0, 0.25]);
        assert!(engine.state.paths.is_empty());
        assert_eq!(engine.state.frames, 0);
        producer.push([0.8, 0.8]).unwrap(); // queued monitor audio is discarded
        let dir = std::env::temp_dir().join(format!(
            "vynull-engine-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        engine.record(dir.clone()).unwrap();
        producer.push([0.5, -0.5]).unwrap();
        producer.push([0.25, -0.25]).unwrap();
        engine.stop().unwrap(); // queued audio must survive a stop between ticks
        assert!(!engine.state.active);
        assert!(!engine.state.recording);
        assert_eq!(engine.state.frames, 2);
        let bytes = std::fs::read(&engine.state.paths[0]).unwrap();
        assert_eq!(bytes.len(), 44 + 12);
        assert_eq!(&bytes[40..44], &12u32.to_le_bytes());
        assert_eq!(&bytes[44..50], &[0, 0, 64, 0, 0, 192]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn overflow_disconnect_and_stalled_input_are_errors_even_during_stop() {
        let (mut engine, _, _) = input();
        engine
            .input
            .as_ref()
            .unwrap()
            .overflow
            .store(true, Ordering::Relaxed);
        assert!(engine.tick().unwrap_err().contains("overrun"));
        let (mut engine, _, errors) = input();
        errors.send("Input disconnected".into()).unwrap();
        assert_eq!(engine.stop().unwrap_err(), "Input disconnected");
        assert!(!engine.state.active);
        let (mut engine, _, _) = input();
        engine.last_audio = Instant::now() - Duration::from_secs(6);
        assert!(engine.tick().unwrap_err().contains("stopped delivering"));
    }

    #[test]
    fn invalid_destination_cannot_enter_recording_state() {
        let (mut engine, _, _) = input();
        assert!(engine.record(PathBuf::from("relative/path")).is_err());
        assert!(!engine.state.recording);
        assert!(engine.state.paths.is_empty());
        assert!(
            Preferences {
                directory: String::new(),
                ..Default::default()
            }
            .directory()
            .is_err()
        );
    }

    #[test]
    fn paths_under_home_show_as_tilde() {
        let home = Some(Path::new("/home/dj"));
        let file = Path::new("/home/dj/Music/Vynull Recordings/Session-1-001.wav");
        assert_eq!(
            home_relative(file, home),
            "~/Music/Vynull Recordings/Session-1-001.wav"
        );
        assert_eq!(
            home_relative(Path::new("/home/dj2/Session.wav"), home),
            "/home/dj2/Session.wav"
        );
        assert_eq!(home_relative(file, None), file.display().to_string());
        assert_eq!(
            home_relative(file, Some(Path::new("/"))),
            file.display().to_string()
        );
    }

    #[test]
    fn missing_explicit_device_never_falls_back_to_default() {
        assert!(
            input::open(
                Some(&Device {
                    id: "not-a-device-id".into(),
                    name: "Missing input".into(),
                    host: "unknown".into()
                }),
                SampleRate::Auto
            )
            .is_err()
        );
    }
}
