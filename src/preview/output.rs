// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from vynull-desktop's native preview output selection.
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, cpal};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OutputDevice {
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

pub fn devices() -> Result<Vec<OutputDevice>, String> {
    let mut devices = vec![];
    let mut errors = vec![];
    for id in hosts() {
        match cpal::host_from_id(id)
            .map_err(|e| e.to_string())
            .and_then(|host| host.output_devices().map_err(|e| e.to_string()))
        {
            Ok(outputs) => {
                for device in outputs.filter(|d| d.supports_output()) {
                    if let Ok(id) = device.id() {
                        devices.push(OutputDevice {
                            id: id.to_string(),
                            name: device.to_string(),
                            host: id.host().name().into(),
                        });
                    }
                }
            }
            Err(e) => errors.push(e),
        }
    }
    devices.sort_by(|a, b| (&a.host, &a.name, &a.id).cmp(&(&b.host, &b.name, &b.id)));
    devices.dedup_by(|a, b| a.id == b.id);
    if devices.is_empty() && !errors.is_empty() {
        Err(errors.join("; "))
    } else {
        Ok(devices)
    }
}

pub struct Output {
    pub sink: MixerDeviceSink,
    pub fault: Arc<AtomicBool>,
    pub name: String,
}

pub fn open(selected: Option<&OutputDevice>) -> Result<Output, String> {
    if let Some(selected) = selected {
        let id = selected
            .id
            .parse::<cpal::DeviceId>()
            .map_err(|e| e.to_string())?;
        let host = cpal::host_from_id(id.host()).map_err(|e| e.to_string())?;
        let device = host
            .device_by_id(&id)
            .filter(|d| d.supports_output())
            .ok_or_else(|| {
                format!(
                    "Output '{}' is unavailable. Choose another output.",
                    selected.name
                )
            })?;
        // An explicit selection never silently falls back to another device.
        return open_device(device);
    }
    let mut errors = vec![];
    for id in hosts() {
        match cpal::host_from_id(id)
            .map_err(|e| e.to_string())
            .and_then(|h| h.default_output_device().ok_or("No default output".into()))
            .and_then(open_device)
        {
            Ok(output) => return Ok(output),
            Err(e) => errors.push(format!("{}: {e}", id.name())),
        }
    }
    Err(format!("Cannot open audio output: {}", errors.join("; ")))
}

fn open_device(device: cpal::Device) -> Result<Output, String> {
    let name = device.to_string();
    let fault = Arc::new(AtomicBool::new(false));
    let callback_fault = fault.clone();
    let mut sink = DeviceSinkBuilder::from_device(device)
        .map_err(|e| e.to_string())?
        .with_error_callback(move |error| {
            if !matches!(
                error.kind(),
                cpal::ErrorKind::Xrun
                    | cpal::ErrorKind::RealtimeDenied
                    | cpal::ErrorKind::DeviceChanged
            ) {
                callback_fault.store(true, Ordering::Relaxed);
            }
        })
        .open_sink_or_fallback()
        .map_err(|e| e.to_string())?;
    sink.log_on_drop(false);
    Ok(Output { sink, fault, name })
}
