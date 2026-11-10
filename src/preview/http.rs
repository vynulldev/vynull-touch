// SPDX-License-Identifier: GPL-3.0-or-later
use super::wav::Pcm;
use reqwest::{Client, Method, Url, header};
use std::{ops::Range, time::Duration};

const SESSION: &str = "X-Vynull-Library-Session";

pub struct AudioFile {
    client: Client,
    url: Url,
    session: String,
    len: u64,
    modified: Option<String>,
    pub pcm: Pcm,
}

impl AudioFile {
    pub async fn open(origin: &str, id: u32, session: Option<&str>) -> Result<Self, String> {
        let session = session
            .filter(|s| !s.is_empty())
            .ok_or("Reconnect to Vynull before previewing tracks; a library session is required")?;
        let origin = crate::api::validate_origin(origin)?;
        let url = origin
            .join(&format!("api/audio/{id}?transcode=1"))
            .map_err(|e| e.to_string())?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        let response = client
            .request(Method::HEAD, url.clone())
            .header(SESSION, session)
            .header(header::ACCEPT_ENCODING, "identity")
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(|e| format!("Cannot prepare audio: {e}"))?;
        check(&response, session)?;
        let len = response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&len| len >= 44)
            .ok_or("Audio response has no valid WAV length")?;
        let modified = response
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let mut file = Self {
            client,
            url,
            session: session.into(),
            len,
            modified,
            pcm: Pcm {
                channels: 2,
                sample_rate: 48_000,
                data_offset: 0,
                frames: 0,
            },
        };
        let header = file.range(0..len.min(65_536)).await?;
        file.pcm = Pcm::parse(&header, len).map_err(|e| e.to_string())?;
        Ok(file)
    }

    async fn range(&self, range: Range<u64>) -> Result<Vec<u8>, String> {
        if range.start >= range.end || range.end > self.len || range.end - range.start > 1024 * 1024
        {
            return Err("Invalid audio byte range".into());
        }
        let mut request = self
            .client
            .get(self.url.clone())
            .header(SESSION, &self.session)
            .header(header::ACCEPT_ENCODING, "identity")
            .header(
                header::RANGE,
                format!("bytes={}-{}", range.start, range.end - 1),
            )
            .timeout(Duration::from_secs(10));
        if let Some(modified) = &self.modified {
            request = request.header(header::IF_UNMODIFIED_SINCE, modified);
        }
        let mut response = request
            .send()
            .await
            .map_err(|e| format!("Cannot read audio: {e}"))?;
        check(&response, &self.session)?;
        let expected = format!("bytes {}-{}/{}", range.start, range.end - 1, self.len);
        if response.status() != reqwest::StatusCode::PARTIAL_CONTENT
            || response
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                != Some(expected.as_str())
        {
            return Err("Vynull returned an unexpected audio range; reopen Inspect".into());
        }
        let len = (range.end - range.start) as usize;
        let mut bytes = Vec::with_capacity(len);
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len() + chunk.len() > len {
                return Err("Oversized audio response".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() != len {
            return Err("Truncated audio response".into());
        }
        Ok(bytes)
    }

    pub async fn samples(&self, start: u64, count: u64) -> Result<Vec<f32>, String> {
        self.pcm
            .decode(&self.range(self.pcm.byte_range(start, count)).await?)
            .map_err(|e| e.to_string())
    }
}

fn check(response: &reqwest::Response, session: &str) -> Result<(), String> {
    if response
        .headers()
        .get(SESSION)
        .and_then(|v| v.to_str().ok())
        != Some(session)
    {
        return Err("Library changed; reconnect before previewing tracks".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "Audio unavailable: HTTP {}",
            response.status().as_u16()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    async fn fixture(
        status: &str,
        content_range: &str,
        session: &str,
        body: &[u8],
    ) -> (AudioFile, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!(
            "http://{}/api/audio/7?transcode=1",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let file = AudioFile {
            client: Client::new(),
            url,
            session: "one".into(),
            len: 100,
            modified: Some("Thu, 05 Nov 2026 12:00:00 GMT".into()),
            pcm: Pcm {
                channels: 2,
                sample_rate: 48_000,
                data_offset: 44,
                frames: 14,
            },
        };
        let mut reply = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Range: {content_range}\r\nX-Vynull-Library-Session: {session}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        reply.extend(body);
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![];
            while !request.ends_with(b"\r\n\r\n") {
                request.push(socket.read_u8().await.unwrap());
            }
            socket.write_all(&reply).await.unwrap();
            String::from_utf8(request).unwrap().to_lowercase()
        });
        (file, task)
    }
    #[tokio::test]
    async fn prepares_wav_and_seeks_using_sample_frames() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut wav = b"RIFF".to_vec();
            wav.extend(40u32.to_le_bytes());
            wav.extend(b"WAVEfmt ");
            wav.extend(16u32.to_le_bytes());
            wav.extend(1u16.to_le_bytes());
            wav.extend(1u16.to_le_bytes());
            wav.extend(8000u32.to_le_bytes());
            wav.extend(16000u32.to_le_bytes());
            wav.extend(2u16.to_le_bytes());
            wav.extend(16u16.to_le_bytes());
            wav.extend(b"data");
            wav.extend(4u32.to_le_bytes());
            wav.extend([0, 128, 0, 64]);
            for step in 0..3 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![];
                while !request.ends_with(b"\r\n\r\n") {
                    request.push(socket.read_u8().await.unwrap());
                }
                let request = String::from_utf8(request).unwrap().to_lowercase();
                assert!(request.contains("/api/audio/7?transcode=1"));
                assert!(request.contains("x-vynull-library-session: one"));
                let (status, range, body, length) = match step {
                    0 => {
                        assert!(request.starts_with("head "));
                        ("200 OK", "", &wav[0..0], 48)
                    }
                    1 => {
                        assert!(request.contains("range: bytes=0-47"));
                        (
                            "206 Partial Content",
                            "Content-Range: bytes 0-47/48\r\n",
                            wav.as_slice(),
                            48,
                        )
                    }
                    _ => {
                        assert!(request.contains("range: bytes=46-47"));
                        (
                            "206 Partial Content",
                            "Content-Range: bytes 46-47/48\r\n",
                            &wav[46..],
                            2,
                        )
                    }
                };
                socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {length}\r\n{range}X-Vynull-Library-Session: one\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            }
        });
        let file = AudioFile::open(&origin, 7, Some("one")).await.unwrap();
        assert_eq!(file.pcm.duration(), 2.0 / 8000.0);
        assert_eq!(
            file.samples(file.pcm.frame_at(1.0 / 8000.0), 1)
                .await
                .unwrap(),
            [0.5]
        );
        task.await.unwrap();
    }

    #[tokio::test]
    async fn ranges_are_exact_session_bound_and_limited() {
        let (file, task) = fixture(
            "206 Partial Content",
            "bytes 44-47/100",
            "one",
            &[0, 128, 255, 127],
        )
        .await;
        assert_eq!(file.samples(0, 1).await.unwrap(), [-1.0, 32767.0 / 32768.0]);
        let request = task.await.unwrap();
        assert!(request.contains("range: bytes=44-47"));
        assert!(request.contains("x-vynull-library-session: one"));
        assert!(request.contains("if-unmodified-since:"));
        assert!(file.range(0..101).await.is_err());
        for (status, range, session, data) in [
            ("200 OK", "bytes 44-47/100", "one", vec![0; 4]),
            ("206 Partial Content", "bytes 0-3/100", "one", vec![0; 4]),
            ("206 Partial Content", "bytes 44-47/100", "two", vec![0; 4]),
            ("206 Partial Content", "bytes 44-47/100", "one", vec![0; 2]),
            ("206 Partial Content", "bytes 44-47/100", "one", vec![0; 5]),
            ("412 Precondition Failed", "", "one", vec![]),
        ] {
            let (file, task) = fixture(status, range, session, &data).await;
            assert!(file.samples(0, 1).await.is_err());
            task.await.unwrap();
        }
    }
}
