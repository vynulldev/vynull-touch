// SPDX-License-Identifier: GPL-3.0-or-later
//! PCM stereo WAVs, split before RIFF's size limit. Each checkpoint makes the
//! completed audio readable even if power is lost before the normal stop.
use super::BitDepth;
use std::{
    fs::{File, OpenOptions},
    io::{self, BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const PART_BYTES: u64 = 1024 * 1024 * 1024;

pub struct Session {
    file: BufWriter<File>,
    directory: PathBuf,
    stem: String,
    rate: u32,
    depth: BitDepth,
    bytes: u64,
    limit: u64,
    pub frames: u64,
    pub paths: Vec<PathBuf>,
}

impl Session {
    pub fn create(directory: &Path, rate: u32, depth: BitDepth) -> io::Result<Self> {
        if !directory.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Choose an absolute recording folder (or ~/Music/Recordings).",
            ));
        }
        std::fs::create_dir_all(directory)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self::create_named(
            directory,
            rate,
            depth,
            &format!("Session-{timestamp}"),
            PART_BYTES,
        )
    }

    fn create_named(
        directory: &Path,
        rate: u32,
        depth: BitDepth,
        stem: &str,
        limit: u64,
    ) -> io::Result<Self> {
        let frame_bytes = u64::from(depth.frame_bytes());
        let limit = limit / frame_bytes * frame_bytes;
        if limit == 0 || limit > u64::from(u32::MAX - 36) || !(8_000..=384_000).contains(&rate) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid WAV recording format or part size",
            ));
        }
        let path = directory.join(format!("{stem}-001.wav"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut session = Self {
            file: BufWriter::with_capacity(64 * 1024, file),
            directory: directory.into(),
            stem: stem.into(),
            rate,
            depth,
            bytes: 0,
            limit,
            frames: 0,
            paths: vec![path],
        };
        session.checkpoint()?;
        Ok(session)
    }

    pub fn write(&mut self, frames: &[[f32; 2]]) -> io::Result<()> {
        for frame in frames {
            if self.bytes >= self.limit {
                self.checkpoint()?;
                let path =
                    self.directory
                        .join(format!("{}-{:03}.wav", self.stem, self.paths.len() + 1));
                let file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                self.file = BufWriter::with_capacity(64 * 1024, file);
                self.paths.push(path);
                self.bytes = 0;
                self.checkpoint()?;
            }
            let mut bytes = [0u8; 6];
            let sample_bytes = usize::from(self.depth.bits() / 8);
            let scale = (1u32 << (self.depth.bits() - 1)) as f32;
            for (channel, sample) in frame.iter().enumerate() {
                let sample = if sample.is_finite() { *sample } else { 0.0 };
                let pcm = (sample.clamp(-1.0, 1.0) * scale)
                    .round()
                    .clamp(-scale, scale - 1.0) as i32;
                bytes[channel * sample_bytes..(channel + 1) * sample_bytes]
                    .copy_from_slice(&pcm.to_le_bytes()[..sample_bytes]);
            }
            self.file.write_all(&bytes[..sample_bytes * 2])?;
            self.bytes += u64::from(self.depth.frame_bytes());
            self.frames += 1;
        }
        Ok(())
    }

    pub fn checkpoint(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file
            .write_all(&header(self.rate, self.depth, self.bytes as u32))?;
        self.file.flush()?;
        self.file.get_ref().sync_data()?;
        self.file.seek(SeekFrom::Start(44 + self.bytes))?;
        Ok(())
    }
}

fn header(rate: u32, depth: BitDepth, bytes: u32) -> [u8; 44] {
    let mut h = [0; 44];
    h[..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + bytes).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&2u16.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * u32::from(depth.frame_bytes())).to_le_bytes());
    h[32..34].copy_from_slice(&depth.frame_bytes().to_le_bytes());
    h[34..36].copy_from_slice(&depth.bits().to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&bytes.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_files_have_exact_samples_and_headers_and_never_overwrite() {
        let dir = std::env::temp_dir().join(format!(
            "vynull-record-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let mut session = Session::create_named(&dir, 48_000, BitDepth::Pcm24, "test", 12).unwrap();
        session
            .write(&[[0.0, 1.0], [-1.0, 0.5], [f32::NAN, -0.5]])
            .unwrap();
        session.checkpoint().unwrap();
        assert_eq!(session.frames, 3);
        assert_eq!(session.paths.len(), 2);
        let first = std::fs::read(&session.paths[0]).unwrap();
        assert_eq!(&first[..44], &header(48_000, BitDepth::Pcm24, 12));
        assert_eq!(&first[44..], &[0, 0, 0, 255, 255, 127, 0, 0, 128, 0, 0, 64]);
        let second = std::fs::read(&session.paths[1]).unwrap();
        assert_eq!(&second[..44], &header(48_000, BitDepth::Pcm24, 6));
        assert_eq!(&second[44..], &[0, 0, 0, 0, 0, 192]);
        assert!(Session::create_named(&dir, 48_000, BitDepth::Pcm24, "test", 12).is_err());
        assert_eq!(std::fs::read(&session.paths[0]).unwrap(), first);
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn pcm16_has_correct_rate_channel_order_and_frame_aligned_parts() {
        let dir = std::env::temp_dir().join(format!(
            "vynull-record-pcm16-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        // A non-aligned limit must split between stereo frames.
        let mut session = Session::create_named(&dir, 96_000, BitDepth::Pcm16, "test", 11).unwrap();
        session
            .write(&[[2.0, -2.0], [0.5, -0.5], [f32::NAN, 0.25]])
            .unwrap();
        session.checkpoint().unwrap();
        assert_eq!(session.frames, 3);
        assert_eq!(session.paths.len(), 2);
        let first = std::fs::read(&session.paths[0]).unwrap();
        assert_eq!(first.len(), 52);
        assert_eq!(&first[4..8], &44u32.to_le_bytes());
        assert_eq!(&first[20..24], &[1, 0, 2, 0]); // PCM, stereo
        assert_eq!(&first[24..28], &96_000u32.to_le_bytes());
        assert_eq!(&first[28..32], &384_000u32.to_le_bytes());
        assert_eq!(&first[32..36], &[4, 0, 16, 0]); // block alignment, bits
        assert_eq!(&first[40..44], &8u32.to_le_bytes());
        assert_eq!(&first[44..], &[255, 127, 0, 128, 0, 64, 0, 192]);
        let second = std::fs::read(&session.paths[1]).unwrap();
        assert_eq!(&second[40..44], &4u32.to_le_bytes());
        assert_eq!(&second[44..], &[0, 0, 0, 32]);
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn relative_recording_folder_is_rejected() {
        assert!(Session::create(Path::new("relative"), 48_000, BitDepth::Pcm24).is_err());
    }
}
