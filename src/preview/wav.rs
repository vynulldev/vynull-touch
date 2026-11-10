// SPDX-License-Identifier: GPL-3.0-or-later

/// Format returned by Vynull's `?transcode=1` endpoint. This deliberately
/// accepts only PCM16 WAV, sharing ffmpeg's decoded timeline with analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pcm {
    pub channels: u16,
    pub sample_rate: u32,
    pub data_offset: u64,
    pub frames: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("Unsupported or malformed preview WAV: {0}")]
pub struct WavError(&'static str);

impl Pcm {
    pub fn parse(header: &[u8], file_len: u64) -> Result<Self, WavError> {
        if header.len() < 12 || &header[..4] != b"RIFF" || &header[8..12] != b"WAVE" {
            return Err(WavError("expected RIFF/WAVE"));
        }
        let mut offset = 12usize;
        let mut format = None;
        while offset + 8 <= header.len() {
            let id = &header[offset..offset + 4];
            let len =
                u32::from_le_bytes(header[offset + 4..offset + 8].try_into().unwrap()) as usize;
            offset += 8;
            if id == b"data" {
                let (channels, sample_rate) = format.ok_or(WavError("data precedes format"))?;
                let align = u64::from(channels) * 2;
                if len == 0
                    || !(len as u64).is_multiple_of(align)
                    || offset as u64 + len as u64 > file_len
                {
                    return Err(WavError("truncated or empty PCM data"));
                }
                return Ok(Self {
                    channels,
                    sample_rate,
                    data_offset: offset as u64,
                    frames: len as u64 / align,
                });
            }
            if id == b"fmt " {
                let fmt = header
                    .get(offset..offset.saturating_add(len))
                    .ok_or(WavError("truncated format"))?;
                if fmt.len() < 16 {
                    return Err(WavError("short format"));
                }
                let word = |at| u16::from_le_bytes([fmt[at], fmt[at + 1]]);
                let channels = word(2);
                let sample_rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
                let pcm_guid = [
                    1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
                ];
                let pcm = word(0) == 1
                    || (word(0) == 0xfffe
                        && fmt.len() >= 40
                        && word(16) >= 22
                        && word(18) == 16
                        && fmt[24..40] == pcm_guid);
                if !pcm
                    || word(14) != 16
                    || !(1..=8).contains(&channels)
                    || !(8_000..=192_000).contains(&sample_rate)
                    || word(12) != channels * 2
                {
                    return Err(WavError("requires 16-bit PCM, 1–8 channels, 8–192 kHz"));
                }
                format = Some((channels, sample_rate));
            }
            offset = offset
                .checked_add(len)
                .and_then(|v| v.checked_add(len % 2))
                .ok_or(WavError("chunk size overflow"))?;
        }
        Err(WavError("data chunk missing from the first 64 KiB"))
    }

    pub fn duration(self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate)
    }

    pub fn frame_at(self, seconds: f64) -> u64 {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0;
        }
        (seconds * f64::from(self.sample_rate))
            .floor()
            .min(self.frames as f64) as u64
    }

    /// Half-open byte range, with seeks always aligned to a complete frame.
    pub fn byte_range(self, frame: u64, count: u64) -> std::ops::Range<u64> {
        let start = frame.min(self.frames);
        let end = start.saturating_add(count).min(self.frames);
        let align = u64::from(self.channels) * 2;
        self.data_offset + start * align..self.data_offset + end * align
    }

    pub fn decode(self, bytes: &[u8]) -> Result<Vec<f32>, WavError> {
        if !bytes.len().is_multiple_of(usize::from(self.channels) * 2) {
            return Err(WavError("partial PCM frame"));
        }
        Ok(bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header() -> Vec<u8> {
        let mut b =
            b"RIFF\x00\x00\x00\x00WAVEJUNK\x01\x00\x00\x00x\x00fmt \x10\x00\x00\x00".to_vec();
        for value in [1u16, 2] {
            b.extend(value.to_le_bytes());
        }
        b.extend(48_000u32.to_le_bytes());
        b.extend(192_000u32.to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(192_000u32.to_le_bytes());
        b
    }
    #[test]
    fn riff_padding_and_exact_frame_timebase() {
        let b = header();
        let pcm = Pcm::parse(&b, b.len() as u64 + 192_000).unwrap();
        assert_eq!(pcm.data_offset, b.len() as u64);
        assert_eq!(pcm.duration(), 1.0);
        assert_eq!(pcm.byte_range(pcm.frame_at(0.125), 1), 24_054..24_058);
        assert_eq!(pcm.frame_at(5.0), 48_000);
        assert_eq!(pcm.frame_at(f64::NAN), 0);
        assert_eq!(
            pcm.decode(&[0, 128, 255, 127]).unwrap(),
            [-1.0, 32767.0 / 32768.0]
        );
        assert!(pcm.decode(&[0, 0]).is_err());
    }
    #[test]
    fn rejects_truncated_and_non_pcm_wav() {
        let mut b = header();
        assert!(Pcm::parse(&b, b.len() as u64).is_err());
        b[30] = 3; // IEEE float in fmt chunk
        assert!(Pcm::parse(&b, 200_000).is_err());
        assert!(Pcm::parse(b"not audio", 100).is_err());
    }
}
