// SPDX-License-Identifier: GPL-3.0-or-later
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleRate {
    Hz44100,
    Hz48000,
    Hz88200,
    Hz96000,
    Hz176400,
    Hz192000,
    #[default]
    #[serde(other)]
    Auto,
}
impl SampleRate {
    pub const ALL: [Self; 7] = [
        Self::Auto,
        Self::Hz44100,
        Self::Hz48000,
        Self::Hz88200,
        Self::Hz96000,
        Self::Hz176400,
        Self::Hz192000,
    ];
    pub fn hz(self) -> Option<u32> {
        match self {
            Self::Auto => None,
            Self::Hz44100 => Some(44_100),
            Self::Hz48000 => Some(48_000),
            Self::Hz88200 => Some(88_200),
            Self::Hz96000 => Some(96_000),
            Self::Hz176400 => Some(176_400),
            Self::Hz192000 => Some(192_000),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Hz44100 => "44.1 kHz",
            Self::Hz48000 => "48 kHz",
            Self::Hz88200 => "88.2 kHz",
            Self::Hz96000 => "96 kHz",
            Self::Hz176400 => "176.4 kHz",
            Self::Hz192000 => "192 kHz",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitDepth {
    Pcm16,
    #[default]
    #[serde(other)]
    Pcm24,
}
impl BitDepth {
    pub fn bits(self) -> u16 {
        match self {
            Self::Pcm16 => 16,
            Self::Pcm24 => 24,
        }
    }
    pub fn frame_bytes(self) -> u16 {
        self.bits() / 8 * 2
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Pcm16 => "16-bit",
            Self::Pcm24 => "24-bit",
        }
    }
}
