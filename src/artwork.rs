// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    api::validate_origin,
    model::Track,
    waveform::{Overview, Style},
};
use eframe::egui::{ColorImage, Context, TextureHandle, TextureId, TextureOptions};
use reqwest::{StatusCode, Url, blocking::Client};
use std::{
    collections::HashMap,
    io::{Cursor, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

const CAPACITY: usize = 128;
const THUMBNAIL_SIZE: u32 = 96;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const SESSION: &str = "X-Vynull-Library-Session";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Artwork(u32),
    Track(u32),
    Waveform(u32, Style, Overview),
}

struct Request {
    key: Key,
    track_id: u32,
    token: u64,
}

struct Reply {
    key: Key,
    token: u64,
    image: Result<Option<ColorImage>, String>,
}

enum State {
    Loading,
    Ready(TextureHandle),
    Retry(Instant),
}

struct Entry {
    state: State,
    used: u64,
    token: u64,
}

/// One cache per connection/library snapshot. Image work never blocks the API worker.
pub struct Cache {
    entries: HashMap<Key, Entry>,
    clock: u64,
    requests: SyncSender<Request>,
    replies: Receiver<Reply>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for Cache {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Cache {
    pub fn new(origin: &str, session: Option<String>, ctx: Context) -> Self {
        // At most one request waiting behind the active download when scrolling quickly.
        let (requests, incoming) = mpsc::sync_channel::<Request>(1);
        let (outgoing, replies) = mpsc::sync_channel(2);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let origin = origin.to_owned();
        thread::spawn(move || {
            let fetcher = Fetcher::new(&origin, session);
            while let Ok(request) = incoming.recv() {
                if worker_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let image = match &fetcher {
                    Ok(fetcher) => match request.key {
                        Key::Waveform(id, style, overview) => fetcher.waveform(id, style, overview),
                        _ => fetcher.fetch(request.track_id),
                    },
                    Err(error) => Err(error.clone()),
                };
                if worker_cancelled.load(Ordering::Relaxed)
                    || outgoing
                        .send(Reply {
                            key: request.key,
                            token: request.token,
                            image,
                        })
                        .is_err()
                {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            entries: HashMap::new(),
            clock: 0,
            requests,
            replies,
            cancelled,
        }
    }

    pub fn poll(&mut self, ctx: &Context) {
        while let Ok(reply) = self.replies.try_recv() {
            if let Some(entry) = self.entries.get_mut(&reply.key)
                && entry.token == reply.token
            {
                entry.state = match reply.image {
                    Ok(Some(image)) => State::Ready(ctx.load_texture(
                        format!("cover-{:?}", reply.key),
                        image,
                        TextureOptions::LINEAR,
                    )),
                    // A 404 can also mean another client is still extracting the cover.
                    Ok(None) => State::Retry(Instant::now() + Duration::from_secs(30)),
                    Err(_) => State::Retry(Instant::now() + Duration::from_secs(10)),
                };
            }
        }
    }

    pub fn invalidate(&mut self, track: &Track) {
        self.entries.retain(|key, _| match *key {
            Key::Artwork(id) => id != track.art_id,
            Key::Track(id) | Key::Waveform(id, _, _) => id != track.id,
        });
    }

    pub fn invalidate_waveforms(&mut self) {
        self.entries
            .retain(|key, _| !matches!(key, Key::Waveform(..)));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Call only for visible rows; albums sharing an art ID share one texture.
    pub fn texture(&mut self, track: &Track) -> Option<TextureId> {
        if track.id == 0 || (track.art_checked && track.art_id == 0) {
            return None;
        }
        let key = if track.art_id == 0 {
            Key::Track(track.id)
        } else {
            Key::Artwork(track.art_id)
        };
        self.request(key, track.id)
    }

    /// Small server-rendered previews share the bounded thumbnail worker. The
    /// palette and shape are part of the key, so changing Setup takes effect immediately.
    pub fn waveform(
        &mut self,
        track: &Track,
        style: Style,
        overview: Overview,
    ) -> Option<TextureId> {
        if track.id == 0 || track.file_missing {
            return None;
        }
        self.request(Key::Waveform(track.id, style, overview), track.id)
    }

    fn request(&mut self, key: Key, track_id: u32) -> Option<TextureId> {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.clock;
            match &entry.state {
                State::Ready(texture) => return Some(texture.id()),
                State::Loading => return None,
                State::Retry(at) if *at > Instant::now() => return None,
                State::Retry(_) => (),
            }
        }
        if self
            .requests
            .try_send(Request {
                key,
                track_id,
                token: self.clock,
            })
            .is_ok()
        {
            if self.entries.len() >= CAPACITY
                && !self.entries.contains_key(&key)
                && let Some(oldest) = self
                    .entries
                    .iter()
                    .filter(|(_, entry)| !matches!(entry.state, State::Loading))
                    .min_by_key(|(_, entry)| entry.used)
                    .map(|(key, _)| *key)
            {
                self.entries.remove(&oldest);
            }
            self.entries.insert(
                key,
                Entry {
                    state: State::Loading,
                    used: self.clock,
                    token: self.clock,
                },
            );
        }
        None
    }
}

pub(crate) struct Fetcher {
    http: Client,
    origin: Url,
    session: Option<String>,
}

impl Fetcher {
    pub(crate) fn new(origin: &str, session: Option<String>) -> Result<Self, String> {
        Ok(Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!(
                    env!("CARGO_PKG_NAME"),
                    "/",
                    env!("CARGO_PKG_VERSION")
                ))
                .build()
                .map_err(|e| e.to_string())?,
            origin: validate_origin(origin)?,
            session,
        })
    }

    fn fetch(&self, track_id: u32) -> Result<Option<ColorImage>, String> {
        self.fetch_path(&format!("/api/artwork/{track_id}"))
    }

    pub(crate) fn fetch_path(&self, path: &str) -> Result<Option<ColorImage>, String> {
        self.fetch_image(path, false)
    }

    fn waveform(
        &self,
        id: u32,
        style: Style,
        overview: Overview,
    ) -> Result<Option<ColorImage>, String> {
        let color = match style {
            Style::Auto | Style::ThreeBand => "3band",
            Style::Rgb => "rgb",
            Style::Blue => "blue",
        };
        // The server always draws native 3-band as Half. Auto's RGB fallback
        // uses the saved shape; explicit 3-band keeps its Half-only contract.
        let shape = if style == Style::ThreeBand || overview == Overview::Half {
            "half"
        } else {
            "full"
        };
        self.fetch_image(&format!("/api/analysis/waveform-png/{id}?type=detail&color={color}&overview={shape}&w=320&h=80"), true)
    }

    fn fetch_image(&self, path: &str, waveform: bool) -> Result<Option<ColorImage>, String> {
        let url = self.origin.join(path).map_err(|e| e.to_string())?;
        let mut request = self.http.get(url);
        if let Some(session) = &self.session {
            request = request.header(SESSION, session);
        }
        let response = request.send().map_err(|e| e.to_string())?;
        if let Some(session) = &self.session
            && response
                .headers()
                .get(SESSION)
                .and_then(|v| v.to_str().ok())
                != Some(session.as_str())
        {
            return Err("Library changed while fetching artwork".into());
        }
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = response.error_for_status().map_err(|e| e.to_string())?;
        if response.content_length().is_some_and(|len| len > MAX_BYTES) {
            return Err("Artwork exceeds download limit".into());
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Artwork exceeds download limit".into());
        }
        if waveform {
            let image = decode_image(&bytes)?;
            // Vynull returns a 1×1 placeholder while analysis is queued. Retry
            // later instead of caching that placeholder as a finished waveform.
            if image.width() <= 1 || image.height() <= 1 {
                return Ok(None);
            }
            Ok(Some(color_image(image.thumbnail(320, 80))))
        } else {
            decode(&bytes).map(Some)
        }
    }
}

fn decode_image(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|e| e.to_string())
}

fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    Ok(color_image(decode_image(bytes)?.resize_to_fill(
        THUMBNAIL_SIZE,
        THUMBNAIL_SIZE,
        image::imageops::FilterType::Triangle,
    )))
}

fn color_image(image: image::DynamicImage) -> ColorImage {
    let image = image.to_rgba8();
    ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener};

    fn encoded(format: image::ImageFormat, width: u32, height: u32) -> Vec<u8> {
        let pixels = image::RgbImage::from_pixel(width, height, image::Rgb([220, 80, 40]));
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(pixels)
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn thumbnails_decode_jpeg_and_png_and_bound_image_dimensions() {
        for format in [image::ImageFormat::Jpeg, image::ImageFormat::Png] {
            let image = decode(&encoded(format, 180, 120)).unwrap();
            assert_eq!(image.size, [96, 96]);
            assert!(image.pixels[0].r() > 200);
        }
        assert!(decode(b"not an image").is_err());
        assert!(decode(&encoded(image::ImageFormat::Png, 4097, 1)).is_err());
    }

    #[test]
    fn artwork_requests_are_session_bound_and_handle_missing_covers() {
        for (status, session, body, expected) in [
            (
                "200 OK",
                "one",
                encoded(image::ImageFormat::Jpeg, 32, 32),
                1,
            ),
            ("404 Not Found", "one", Vec::new(), 0),
            (
                "200 OK",
                "two",
                encoded(image::ImageFormat::Png, 32, 32),
                -1,
            ),
            ("200 OK", "one", b"bad image".to_vec(), -1),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let fetcher = Fetcher::new(
                &format!("http://{}", listener.local_addr().unwrap()),
                Some("one".into()),
            )
            .unwrap();
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                write!(socket, "HTTP/1.1 {status}\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                socket.write_all(&body).unwrap();
                String::from_utf8(request).unwrap().to_lowercase()
            });
            match (expected, fetcher.fetch(42)) {
                (1, Ok(Some(image))) => assert_eq!(image.size, [96, 96]),
                (0, Ok(None)) | (-1, Err(_)) => (),
                (_, other) => panic!("unexpected artwork response: {other:?}"),
            }
            let request = server.join().unwrap();
            assert!(request.starts_with("get /api/artwork/42 "));
            assert!(request.contains("x-vynull-library-session: one"));
        }
    }

    fn cache_fixture() -> (Cache, Receiver<Request>, SyncSender<Reply>) {
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, replies) = mpsc::sync_channel(2);
        (
            Cache {
                entries: HashMap::new(),
                clock: 0,
                requests,
                replies,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
            incoming,
            outgoing,
        )
    }

    #[test]
    fn invalidated_images_reject_inflight_replies_for_reused_keys() {
        for waveform in [false, true] {
            let ctx = Context::default();
            let (mut cache, requests, replies) = cache_fixture();
            let track = Track {
                id: 1,
                art_id: 5,
                ..Default::default()
            };
            let fetch = |cache: &mut Cache| {
                if waveform {
                    cache.waveform(&track, Style::Rgb, Overview::Half)
                } else {
                    cache.texture(&track)
                }
            };
            fetch(&mut cache);
            let old = requests.try_recv().unwrap();
            cache.invalidate(&track);
            fetch(&mut cache);
            let new = requests.try_recv().unwrap();
            assert_eq!(old.key, new.key);
            assert_ne!(old.token, new.token);
            replies
                .send(Reply {
                    key: old.key,
                    token: old.token,
                    image: Ok(None),
                })
                .unwrap();
            cache.poll(&ctx);
            assert!(matches!(cache.entries[&new.key].state, State::Loading));
            replies
                .send(Reply {
                    key: new.key,
                    token: new.token,
                    image: Ok(Some(
                        decode(&encoded(image::ImageFormat::Png, 8, 8)).unwrap(),
                    )),
                })
                .unwrap();
            cache.poll(&ctx);
            assert!(fetch(&mut cache).is_some());
        }
    }

    #[test]
    fn waveform_previews_use_small_session_bound_images_and_retry_placeholders() {
        for (style, overview, color, shape, session, dimensions, expected) in [
            (
                Style::Auto,
                Overview::Full,
                "3band",
                "full",
                "one",
                (320, 80),
                1,
            ),
            (
                Style::Rgb,
                Overview::Half,
                "rgb",
                "half",
                "one",
                (320, 80),
                1,
            ),
            (
                Style::Blue,
                Overview::Full,
                "blue",
                "full",
                "one",
                (320, 80),
                1,
            ),
            (
                Style::ThreeBand,
                Overview::Full,
                "3band",
                "half",
                "one",
                (320, 80),
                1,
            ),
            (
                Style::Auto,
                Overview::Half,
                "3band",
                "half",
                "one",
                (1, 1),
                0,
            ),
            (
                Style::Rgb,
                Overview::Full,
                "rgb",
                "full",
                "two",
                (320, 80),
                -1,
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let fetcher = Fetcher::new(
                &format!("http://{}", listener.local_addr().unwrap()),
                Some("one".into()),
            )
            .unwrap();
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let body = encoded(image::ImageFormat::Png, dimensions.0, dimensions.1);
                write!(socket, "HTTP/1.1 200 OK\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                socket.write_all(&body).unwrap();
                String::from_utf8(request).unwrap().to_lowercase()
            });
            match (expected, fetcher.waveform(42, style, overview)) {
                (1, Ok(Some(image))) => assert_eq!(
                    image.size,
                    [320, 80],
                    "waveform must not be cropped to a square"
                ),
                (0, Ok(None)) | (-1, Err(_)) => (),
                (_, other) => panic!("unexpected waveform response: {other:?}"),
            }
            let request = server.join().unwrap();
            assert!(request.starts_with(&format!("get /api/analysis/waveform-png/42?type=detail&color={color}&overview={shape}&w=320&h=80 ")));
            assert!(request.contains("x-vynull-library-session: one"));
        }
    }

    #[test]
    fn waveform_cache_keys_include_track_palette_and_shape() {
        let ctx = Context::default();
        let (mut cache, requests, replies) = cache_fixture();
        let track = Track {
            id: 1,
            art_id: 5,
            ..Default::default()
        };
        let mut keys = std::collections::HashSet::new();
        for (id, style, shape) in [
            (1, Style::Rgb, Overview::Full),
            (1, Style::Rgb, Overview::Half),
            (1, Style::Blue, Overview::Half),
            (2, Style::Blue, Overview::Half),
        ] {
            let track = Track {
                id,
                ..track.clone()
            };
            assert!(cache.waveform(&track, style, shape).is_none());
            let request = requests.try_recv().unwrap();
            assert!(keys.insert(request.key));
            replies
                .send(Reply {
                    key: request.key,
                    token: request.token,
                    image: Ok(None),
                })
                .unwrap();
            cache.poll(&ctx);
            assert!(cache.waveform(&track, style, shape).is_none());
            assert!(
                requests.try_recv().is_err(),
                "unfinished analysis must not retry every frame"
            );
        }
        cache.texture(&track);
        assert!(
            keys.insert(requests.try_recv().unwrap().key),
            "album art and waveform must not share a key"
        );
        assert!(
            cache
                .waveform(
                    &Track {
                        file_missing: true,
                        ..track
                    },
                    Style::Auto,
                    Overview::Full
                )
                .is_none()
        );
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn cache_shares_album_covers_and_throttles_missing_artwork() {
        let ctx = Context::default();
        let (mut cache, requests, replies) = cache_fixture();
        let mut track = Track {
            id: 1,
            art_id: 50,
            ..Default::default()
        };
        assert!(cache.texture(&track).is_none());
        let request = requests.try_recv().unwrap();
        track.id = 2;
        assert!(cache.texture(&track).is_none());
        assert!(
            requests.try_recv().is_err(),
            "same album should not download twice"
        );
        replies
            .send(Reply {
                key: request.key,
                token: request.token,
                image: Ok(Some(
                    decode(&encoded(image::ImageFormat::Png, 8, 8)).unwrap(),
                )),
            })
            .unwrap();
        cache.poll(&ctx);
        let texture = cache.texture(&track).unwrap();
        track.id = 1;
        assert_eq!(cache.texture(&track), Some(texture));

        track.art_id = 0;
        track.art_checked = true;
        assert!(cache.texture(&track).is_none());
        assert!(
            requests.try_recv().is_err(),
            "known missing art should not be requested"
        );
        track.art_checked = false;
        assert!(cache.texture(&track).is_none());
        let request = requests.try_recv().unwrap();
        replies
            .send(Reply {
                key: request.key,
                token: request.token,
                image: Ok(None),
            })
            .unwrap();
        cache.poll(&ctx);
        for _ in 0..5 {
            assert!(cache.texture(&track).is_none());
        }
        assert!(
            requests.try_recv().is_err(),
            "404 must not retry every frame"
        );
        cache.entries.get_mut(&request.key).unwrap().state = State::Retry(Instant::now());
        assert!(cache.texture(&track).is_none());
        assert_eq!(requests.try_recv().unwrap().track_id, 1);
    }

    #[test]
    fn cache_bounds_queue_and_evicts_least_recently_used_covers() {
        let ctx = Context::default();
        let (mut cache, requests, replies) = cache_fixture();
        let track = |id| Track {
            id,
            art_id: id,
            ..Default::default()
        };
        cache.texture(&track(1));
        cache.texture(&track(2));
        assert!(
            !cache.entries.contains_key(&Key::Artwork(2)),
            "full queue must not leave a request stuck as loading"
        );
        let request = requests.try_recv().unwrap();
        replies
            .send(Reply {
                key: request.key,
                token: request.token,
                image: Ok(None),
            })
            .unwrap();
        cache.poll(&ctx);
        for id in 2..=CAPACITY as u32 {
            cache.texture(&track(id));
            let request = requests.try_recv().unwrap();
            replies
                .send(Reply {
                    key: request.key,
                    token: request.token,
                    image: Ok(None),
                })
                .unwrap();
            cache.poll(&ctx);
        }
        cache.texture(&track(1)); // keep the oldest cover in use
        cache.texture(&track(CAPACITY as u32 + 1));
        assert_eq!(cache.entries.len(), CAPACITY);
        assert!(cache.entries.contains_key(&Key::Artwork(1)));
        assert!(!cache.entries.contains_key(&Key::Artwork(2)));
    }
}
