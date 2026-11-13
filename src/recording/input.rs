// SPDX-License-Identifier: GPL-3.0-or-later
use super::SampleRate;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub host: String,
}

fn hosts() -> Vec<cpal::HostId> {
    #[cfg(target_os = "linux")]
    {
        vec![cpal::HostId::PulseAudio, cpal::HostId::Alsa]
    }
    #[cfg(not(target_os = "linux"))]
    {
        vec![cpal::default_host().id()]
    }
}

pub fn devices() -> Result<Vec<Device>, String> {
    let mut result = vec![];
    let mut errors = vec![];
    for id in hosts() {
        match cpal::host_from_id(id).and_then(|h| h.input_devices()) {
            Ok(devices) => {
                for device in devices.filter(|d| d.supports_input()) {
                    if let Ok(id) = device.id() {
                        result.push(Device {
                            id: id.to_string(),
                            name: device.to_string(),
                            host: id.host().name().into(),
                        });
                    }
                }
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    result.sort_by(|a, b| (&a.host, &a.name).cmp(&(&b.host, &b.name)));
    result.dedup_by(|a, b| a.id == b.id);
    if result.is_empty() && !errors.is_empty() {
        Err(errors.join("; "))
    } else {
        Ok(result)
    }
}

pub struct Input {
    pub stream: Option<cpal::Stream>,
    pub frames: rtrb::Consumer<[f32; 2]>,
    pub overflow: Arc<AtomicBool>,
    pub errors: mpsc::Receiver<String>,
    pub name: String,
    pub rate: u32,
}

pub fn open(selected: Option<&Device>, rate: SampleRate) -> Result<Input, String> {
    // Some backends wait for the first sample inside play(). Bound the whole
    // open operation so a stalled audio service cannot trap the UI in Starting.
    let selected = selected.cloned();
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = tx.send(open_inner(selected.as_ref(), rate));
    });
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| {
            "Timed out opening the audio input. Check the device and audio service.".to_owned()
        })?
}

fn open_inner(selected: Option<&Device>, rate: SampleRate) -> Result<Input, String> {
    if let Some(selected) = selected {
        let id = selected
            .id
            .parse::<cpal::DeviceId>()
            .map_err(|e| e.to_string())?;
        let host = cpal::host_from_id(id.host()).map_err(|e| e.to_string())?;
        let device = host
            .device_by_id(&id)
            .filter(|d| d.supports_input())
            .ok_or_else(|| {
                format!(
                    "Input '{}' is unavailable. Choose another input.",
                    selected.name
                )
            })?;
        return open_device(device, rate);
    }
    let mut errors = vec![];
    for id in hosts() {
        match cpal::host_from_id(id)
            .map_err(|e| e.to_string())
            .and_then(|h| h.default_input_device().ok_or("No default input".into()))
            .and_then(|device| open_device(device, rate))
        {
            Ok(input) => return Ok(input),
            Err(e) => errors.push(format!("{}: {e}", id.name())),
        }
    }
    Err(format!("Cannot open input: {}", errors.join("; ")))
}

fn choose_config(
    configs: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
    default: Option<cpal::SupportedStreamConfig>,
    rate: SampleRate,
) -> Result<cpal::SupportedStreamConfig, String> {
    configs
        .filter(|c| c.channels() == 2 && supported(c.sample_format()))
        .filter_map(|c| c.try_with_sample_rate(rate.hz().unwrap_or(48_000)))
        .max_by_key(|c| c.sample_format() == cpal::SampleFormat::F32)
        .or_else(|| {
            default.filter(|c| {
                c.channels() == 2
                    && supported(c.sample_format())
                    && rate
                        .hz()
                        .is_none_or(|requested| c.sample_rate() == requested)
            })
        })
        .ok_or_else(|| match rate.hz() {
            Some(_) => format!(
                "This input does not support stereo at {}. Choose another rate or Auto.",
                rate.label()
            ),
            None => {
                "This input does not offer a supported stereo format. Choose a stereo input.".into()
            }
        })
}

fn open_device(device: cpal::Device, requested_rate: SampleRate) -> Result<Input, String> {
    let configs = device
        .supported_input_configs()
        .map(|c| c.collect::<Vec<_>>())
        .unwrap_or_default();
    let config = choose_config(
        configs.into_iter(),
        device.default_input_config().ok(),
        requested_rate,
    )?;
    let rate = config.sample_rate();
    if !(8_000..=384_000).contains(&rate) {
        return Err("Unsupported input sample rate".into());
    }
    let format = config.sample_format();
    // Desktop servers can otherwise default to multi-second capture fragments.
    // Request roughly 20 ms so meters and the Record/Stop boundary stay responsive.
    let mut stream_config = config.config();
    let frames_per_callback = match config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => (rate / 50).clamp(*min, *max),
        cpal::SupportedBufferSize::Unknown => rate / 50,
    };
    stream_config.buffer_size = cpal::BufferSize::Fixed(frames_per_callback);
    // Whole stereo frames keep the channels aligned even if the writer stalls.
    // The callback never allocates, waits for disk, or waits for the UI.
    let (mut producer, frames) = rtrb::RingBuffer::new(rate as usize * 2);
    let overflow = Arc::new(AtomicBool::new(false));
    let fault = overflow.clone();
    let (errors_tx, errors) = mpsc::sync_channel(1);
    let stream = device
        .build_input_stream_raw(
            stream_config,
            format,
            move |data, _| {
                macro_rules! capture {
                    ($sample:ty) => {
                        if let Some(samples) = data.as_slice::<$sample>() {
                            for pair in samples.chunks_exact(2) {
                                use cpal::Sample;
                                let frame =
                                    [pair[0].to_sample::<f32>(), pair[1].to_sample::<f32>()];
                                if producer.push(frame).is_err() {
                                    fault.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                    };
                }
                match format {
                    cpal::SampleFormat::F32 => capture!(f32),
                    cpal::SampleFormat::F64 => capture!(f64),
                    cpal::SampleFormat::I16 => capture!(i16),
                    cpal::SampleFormat::I24 => capture!(cpal::I24),
                    cpal::SampleFormat::I32 => capture!(i32),
                    cpal::SampleFormat::U16 => capture!(u16),
                    _ => (), // Configuration was validated before opening the stream.
                }
            },
            move |e| {
                if !matches!(e.kind(), cpal::ErrorKind::RealtimeDenied) {
                    let _ = errors_tx.try_send(format!("Audio input interrupted: {e}"));
                }
            },
            Some(std::time::Duration::from_secs(5)),
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Input {
        stream: Some(stream),
        frames,
        overflow,
        errors,
        name: device.to_string(),
        rate,
    })
}

fn supported(format: cpal::SampleFormat) -> bool {
    matches!(
        format,
        cpal::SampleFormat::F32
            | cpal::SampleFormat::F64
            | cpal::SampleFormat::I16
            | cpal::SampleFormat::I24
            | cpal::SampleFormat::I32
            | cpal::SampleFormat::U16
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};

    fn range(
        channels: u16,
        min: u32,
        max: u32,
        format: SampleFormat,
    ) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(channels, min, max, SupportedBufferSize::Unknown, format)
    }

    #[test]
    fn requested_rate_is_exact_and_prefers_float_capture() {
        let configs = [
            range(2, 44_100, 192_000, SampleFormat::F32),
            range(2, 44_100, 192_000, SampleFormat::I16),
        ];
        for rate in SampleRate::ALL {
            let chosen = choose_config(configs.into_iter(), None, rate).unwrap();
            assert_eq!(chosen.sample_rate(), rate.hz().unwrap_or(48_000));
            assert_eq!(chosen.sample_format(), SampleFormat::F32);
        }
    }

    #[test]
    fn only_auto_may_fall_back_to_a_different_default_rate() {
        let cd = range(2, 44_100, 44_100, SampleFormat::I16);
        let default = cd.with_max_sample_rate();
        let chosen = choose_config([cd].into_iter(), Some(default), SampleRate::Auto).unwrap();
        assert_eq!(chosen.sample_rate(), 44_100);
        let error =
            choose_config([cd].into_iter(), Some(default), SampleRate::Hz96000).unwrap_err();
        assert!(error.contains("96 kHz"));
        assert_eq!(
            choose_config(std::iter::empty(), Some(default), SampleRate::Hz44100)
                .unwrap()
                .sample_rate(),
            44_100
        );
    }

    #[test]
    fn mono_and_unsupported_sample_formats_are_rejected() {
        for config in [
            range(1, 8_000, 192_000, SampleFormat::F32),
            range(2, 8_000, 192_000, SampleFormat::I8),
        ] {
            assert!(
                choose_config(
                    [config].into_iter(),
                    Some(config.with_sample_rate(48_000)),
                    SampleRate::Auto
                )
                .is_err()
            );
        }
    }
}
