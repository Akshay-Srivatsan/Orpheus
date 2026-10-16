//! Spotify Premium playback via librespot.
//!
//! Each deck gets its own librespot `Player` whose sink forwards decoded PCM
//! into the deck's pipeline, so Spotify tracks get the same tempo control,
//! fades and routing as local files. Metadata comes from librespot's metadata
//! API; search and "your playlists" use the Web API with a token from the
//! browser sign-in.

use crate::audio::source::Source;
use std::collections::HashMap;
use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, SendTimeoutError, Sender};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::SessionConfig;
use librespot_core::session::Session;
use librespot_core::SpotifyUri;
use librespot_metadata::Metadata;
use librespot_playback::audio_backend::{Sink, SinkResult};
use librespot_playback::config::{Bitrate, PlayerConfig};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use librespot_playback::mixer::NoOpVolume;
use librespot_playback::player::{Player, PlayerEvent};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// librespot's own client id; its registered redirect is http://127.0.0.1:8898/login.
/// Used for playback. Many apps share it, so Spotify rate-limits its Web API
/// use heavily; search and playlists can use your own app instead
/// (`set_web_client_id`).
const DEFAULT_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
/// Redirect for both sign-ins (register it in your own Spotify app).
pub const REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const SCOPES: &[&str] = &[
    "streaming",
    "user-read-private",
    "user-read-email",
    "playlist-read-private",
    "playlist-read-collaborative",
    "user-library-read",
];
const SAMPLE_RATE: u32 = 44100;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyStatus {
    pub connected: bool,
    pub connecting: bool,
    pub username: Option<String>,
    pub error: Option<String>,
    pub has_cached_credentials: bool,
    /// Search and "your playlists" (Spotify Web API) are available.
    pub web_api: bool,
    /// Search and playlists use your own Spotify app (not the shared client id).
    pub own_web_app: bool,
}

/// OAuth token from the browser sign-in, used for the Web API (search,
/// playlists). Stored on this computer only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebToken {
    client_id: String,
    access_token: String,
    refresh_token: String,
    /// Epoch ms.
    expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyTrack {
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyPlaylist {
    pub uri: String,
    pub name: String,
    pub owner: String,
    pub track_count: i64,
    pub artwork_url: Option<String>,
}

enum Msg {
    Audio(u64, Vec<f32>),
    Eof(u64),
    Unavailable(u64),
}

struct ChannelSink {
    tx: Sender<Msg>,
    gen: Arc<AtomicU64>,
}

impl Sink for ChannelSink {
    fn write(&mut self, packet: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
        let AudioPacket::Samples(samples) = packet else { return Ok(()) };
        let gen = self.gen.load(Ordering::SeqCst);
        let mut msg = Msg::Audio(gen, samples.iter().map(|&s| s as f32).collect());
        // Block (this paces librespot) unless the deck has moved on.
        loop {
            match self.tx.send_timeout(msg, Duration::from_millis(100)) {
                Ok(()) => return Ok(()),
                Err(SendTimeoutError::Timeout(m)) => {
                    if self.gen.load(Ordering::SeqCst) != gen {
                        return Ok(());
                    }
                    msg = m;
                }
                Err(SendTimeoutError::Disconnected(_)) => return Ok(()),
            }
        }
    }
}

/// A librespot player bound to one deck.
pub struct DeckPlayer {
    player: Arc<Player>,
    rx: Receiver<Msg>,
    gen: Arc<AtomicU64>,
    /// Serialises loads so a source owns the player until replaced.
    lock: Mutex<()>,
}

impl DeckPlayer {
    fn new(session: Session, rt: &tokio::runtime::Handle) -> Arc<Self> {
        let (tx, rx) = bounded::<Msg>(24);
        let gen = Arc::new(AtomicU64::new(0));
        let sink_gen = gen.clone();
        let sink_tx = tx.clone();
        let config = PlayerConfig { bitrate: Bitrate::Bitrate320, gapless: false, ..Default::default() };
        let _guard = rt.enter();
        let player = Player::new(config, session, Box::new(NoOpVolume), move || {
            Box::new(ChannelSink { tx: sink_tx, gen: sink_gen })
        });
        let mut events = player.get_player_event_channel();
        let ev_gen = gen.clone();
        rt.spawn(async move {
            while let Some(ev) = events.recv().await {
                let g = ev_gen.load(Ordering::SeqCst);
                let msg = match ev {
                    PlayerEvent::EndOfTrack { .. } => Some(Msg::Eof(g)),
                    PlayerEvent::Unavailable { .. } => Some(Msg::Unavailable(g)),
                    _ => None,
                };
                if let Some(m) = msg {
                    // Use a blocking send off the async executor.
                    let tx = tx.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = tx.send_timeout(m, Duration::from_secs(30));
                    })
                    .await;
                }
            }
        });
        Arc::new(Self { player, rx, gen, lock: Mutex::new(()) })
    }
}

pub struct SpotifySource {
    dp: Arc<DeckPlayer>,
    uri: SpotifyUri,
    gen: u64,
    pos_frames: u64,
    duration: Option<f64>,
    eof: bool,
    /// First packet, received while waiting in `open`.
    pending: Option<Vec<f32>>,
}

impl SpotifySource {
    pub fn open(dp: Arc<DeckPlayer>, uri: &str, duration_ms: i64, start_secs: f64) -> Result<Self> {
        let uri = SpotifyUri::from_uri(uri).map_err(|e| anyhow!("invalid Spotify URI: {e}"))?;
        let _l = dp.lock.lock();
        let gen = dp.gen.fetch_add(1, Ordering::SeqCst) + 1;
        dp.player.load(uri.clone(), true, (start_secs * 1000.0) as u32);
        let mut s = Self {
            dp: dp.clone(),
            uri,
            gen,
            pos_frames: (start_secs * SAMPLE_RATE as f64) as u64,
            duration: (duration_ms > 0).then(|| duration_ms as f64 / 1000.0),
            eof: false,
            pending: None,
        };
        // Wait until audio actually arrives so load errors surface here.
        s.wait_first()?;
        Ok(s)
    }

    fn wait_first(&mut self) -> Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Err(anyhow!("timed out loading Spotify track"));
            }
            match self.dp.rx.recv_timeout(left) {
                Ok(Msg::Audio(g, samples)) if g == self.gen => {
                    self.pending = Some(samples);
                    return Ok(());
                }
                Ok(Msg::Unavailable(g)) if g == self.gen => {
                    return Err(anyhow!("track unavailable on Spotify"))
                }
                Ok(Msg::Eof(g)) if g == self.gen => {
                    self.eof = true;
                    return Ok(());
                }
                Ok(_) => {}
                Err(_) => return Err(anyhow!("timed out loading Spotify track")),
            }
        }
    }
}

impl Drop for SpotifySource {
    fn drop(&mut self) {
        // Release the sink if it's blocked on this source.
        if self.dp.gen.load(Ordering::SeqCst) == self.gen {
            self.dp.gen.fetch_add(1, Ordering::SeqCst);
            self.dp.player.stop();
        }
    }
}

impl Source for SpotifySource {
    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }
    fn duration_secs(&self) -> Option<f64> {
        self.duration
    }
    fn read(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        if let Some(p) = self.pending.take() {
            self.pos_frames += (p.len() / 2) as u64;
            out.extend_from_slice(&p);
            return Ok(true);
        }
        if self.eof {
            return Ok(false);
        }
        loop {
            match self.dp.rx.recv_timeout(Duration::from_secs(15)) {
                Ok(Msg::Audio(g, samples)) if g == self.gen => {
                    self.pos_frames += (samples.len() / 2) as u64;
                    out.extend_from_slice(&samples);
                    return Ok(true);
                }
                Ok(Msg::Eof(g)) if g == self.gen => {
                    self.eof = true;
                    return Ok(false);
                }
                Ok(Msg::Unavailable(g)) if g == self.gen => {
                    self.eof = true;
                    return Err(anyhow!("track unavailable"));
                }
                Ok(_) => {} // stale packet from an earlier load/seek
                Err(RecvTimeoutError::Timeout) => return Err(anyhow!("Spotify stream stalled (network?)")),
                Err(RecvTimeoutError::Disconnected) => return Ok(false),
            }
        }
    }
    fn seek(&mut self, secs: f64) -> Result<()> {
        let _l = self.dp.lock.lock();
        self.gen = self.dp.gen.fetch_add(1, Ordering::SeqCst) + 1;
        self.pending = None;
        self.eof = false;
        // Reload rather than seek: works whether or not the previous stream ended.
        self.dp.player.load(self.uri.clone(), true, (secs * 1000.0) as u32);
        self.pos_frames = (secs * SAMPLE_RATE as f64) as u64;
        Ok(())
    }
    fn position_secs(&self) -> f64 {
        self.pos_frames as f64 / SAMPLE_RATE as f64
    }
}

struct Connected {
    session: Session,
    players: [Option<Arc<DeckPlayer>>; 2],
}

pub struct Spotify {
    rt: tokio::runtime::Handle,
    dir: PathBuf,
    conn: Mutex<Option<Connected>>,
    pub status: Mutex<SpotifyStatus>,
    http: reqwest::Client,
    web_token: Mutex<Option<WebToken>>,
    /// Spotify rate-limited us (HTTP 429): no Web API calls until then (epoch ms).
    web_blocked_until: Mutex<i64>,
    /// Recent search results (query → time, results), so repeated searches don't hit the API.
    search_cache: Mutex<HashMap<String, (i64, Vec<SpotifyTrack>)>>,
    /// Client id of your own Spotify app for the Web API ("" = the shared one).
    web_client_id: Mutex<String>,
    /// Largest page size the Web API accepted (see `web_get_limited`).
    page_limit: Mutex<u32>,
}

/// How often the watchdog checks whether Spotify dropped the session.
const SESSION_CHECK: std::time::Duration = std::time::Duration::from_secs(30);

/// Page sizes to try, largest first.
const PAGE_LIMITS: &[u32] = &[50, 20, 10, 5, 1];

fn is_invalid_limit(e: &anyhow::Error) -> bool {
    let m = e.to_string().to_lowercase();
    m.contains("400") && m.contains("limit")
}

/// How long search results are reused.
const SEARCH_CACHE_MS: i64 = 10 * 60_000;
const SEARCH_CACHE_MAX: usize = 200;

fn cover_url(images: &librespot_metadata::image::Images) -> Option<String> {
    // Prefer a medium-size image.
    let imgs = &images.0;
    let pick = imgs.iter().find(|i| i.width >= 200 && i.width <= 400).or_else(|| imgs.first())?;
    Some(format!("https://i.scdn.co/image/{}", pick.id))
}

impl Spotify {
    pub fn new(rt: tokio::runtime::Handle, dir: PathBuf) -> Self {
        let has = dir.join("credentials.json").exists();
        let s = Self {
            rt,
            dir,
            conn: Mutex::new(None),
            status: Mutex::new(SpotifyStatus { has_cached_credentials: has, ..Default::default() }),
            http: reqwest::Client::new(),
            web_token: Mutex::new(None),
            web_blocked_until: Mutex::new(0),
            search_cache: Mutex::new(HashMap::new()),
            web_client_id: Mutex::new(String::new()),
            page_limit: Mutex::new(PAGE_LIMITS[0]),
        };
        *s.web_token.lock() = std::fs::read_to_string(s.web_token_path()).ok().and_then(|t| serde_json::from_str(&t).ok());
        s
    }

    fn web_token_path(&self) -> PathBuf {
        self.dir.join("web-token.json")
    }

    fn save_web_token(&self, t: &WebToken) {
        let _ = std::fs::create_dir_all(&self.dir);
        let path = self.web_token_path();
        if let Ok(json) = serde_json::to_string(t) {
            if crate::store::write_atomic(&path, json.as_bytes()).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                }
            }
        }
        *self.web_token.lock() = Some(t.clone());
    }

    fn cache(&self) -> Result<Cache> {
        Cache::new(Some(self.dir.clone()), None, Some(self.dir.join("audio")), Some(2 << 30))
            .map_err(|e| anyhow!("spotify cache: {e}"))
    }

    pub fn status(&self) -> SpotifyStatus {
        let mut s = self.status.lock().clone();
        s.has_cached_credentials = self.dir.join("credentials.json").exists();
        let cid = self.web_cid();
        s.web_api = self.web_token.lock().as_ref().is_some_and(|t| t.client_id == cid);
        s.own_web_app = cid != DEFAULT_CLIENT_ID;
        s
    }

    pub fn session(&self) -> Option<Session> {
        self.conn.lock().as_ref().map(|c| c.session.clone())
    }

    /// The session, reconnected with the saved login if Spotify dropped it
    /// (it closes idle connections; network changes break them).
    fn live_session(&self) -> Result<Session> {
        self.ensure_connected()?;
        self.session().ok_or_else(|| anyhow!("Spotify is not connected"))
    }

    /// Reconnects with the saved login when a session was open but dropped.
    /// Call from outside the async runtime.
    pub fn ensure_connected(&self) -> Result<()> {
        let state = self.conn.lock().as_ref().map(|c| c.session.is_invalid());
        match state {
            None => Err(anyhow!("Spotify is not connected")),
            Some(false) => Ok(()),
            Some(true) => {
                log::info!("Spotify session dropped; reconnecting");
                // The deck players belong to the old session.
                *self.conn.lock() = None;
                self.connect_cached().map_err(|e| anyhow!("Spotify session expired and reconnecting failed ({e:#}); sign in again in Settings → Spotify"))
            }
        }
    }

    /// Reconnects now and renews the Web API token (e.g. just before a set).
    pub fn refresh(&self) -> Result<()> {
        *self.conn.lock() = None;
        self.connect_cached()?;
        let token = self.web_token.lock().clone();
        if let Some(mut t) = token.filter(|t| !t.refresh_token.is_empty()) {
            t.expires_at = 0;
            *self.web_token.lock() = Some(t);
            self.rt.block_on(self.web_token())?;
        }
        Ok(())
    }

    /// Checks the session every `SESSION_CHECK` and reconnects if it dropped,
    /// so it's ready when a song is loaded; `changed` is told when it reconnects.
    pub fn start_watchdog(self: &Arc<Self>, changed: impl Fn() + Send + 'static) {
        let me = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("spotify-watchdog".into())
            .spawn(move || loop {
                std::thread::sleep(SESSION_CHECK);
                let Some(s) = me.upgrade() else { return };
                if s.conn.lock().as_ref().is_some_and(|c| c.session.is_invalid()) {
                    match s.ensure_connected() {
                        Ok(()) => log::info!("Spotify reconnected"),
                        Err(e) => log::warn!("{e:#}"),
                    }
                    changed();
                }
            })
            .expect("spawn spotify watchdog");
    }

    pub fn is_connected(&self) -> bool {
        self.conn.lock().is_some()
    }

    async fn connect_with(&self, creds: Credentials, client_id: &str) -> Result<()> {
        let cache = self.cache()?;
        let mut config = SessionConfig::default();
        if !client_id.is_empty() {
            config.client_id = client_id.to_string();
        }
        let session = Session::new(config, Some(cache));
        session.connect(creds, true).await.map_err(|e| anyhow!("Spotify login failed: {e}"))?;
        let username = session.username();
        *self.conn.lock() = Some(Connected { session, players: [None, None] });
        let mut st = self.status.lock();
        st.connected = true;
        st.connecting = false;
        st.username = Some(username);
        st.error = None;
        Ok(())
    }

    /// Uses your own Spotify app (client id) for search and playlists; "" = the shared one.
    pub fn set_web_client_id(&self, id: &str) {
        let id = id.trim().to_string();
        let mut cur = self.web_client_id.lock();
        if *cur != id {
            *cur = id;
            // A rate limit or page-size cap on one client id says nothing about another.
            *self.web_blocked_until.lock() = 0;
            *self.page_limit.lock() = PAGE_LIMITS[0];
        }
    }

    fn web_cid(&self) -> String {
        let id = self.web_client_id.lock().clone();
        if id.is_empty() { DEFAULT_CLIENT_ID.to_string() } else { id }
    }

    /// Connects with cached credentials (no browser).
    pub fn connect_cached(&self) -> Result<()> {
        let creds = self.cache()?.credentials().ok_or_else(|| anyhow!("no saved Spotify login"))?;
        self.status.lock().connecting = true;
        let r = self.rt.block_on(self.connect_with(creds, ""));
        if let Err(e) = &r {
            let mut st = self.status.lock();
            st.connecting = false;
            st.error = Some(format!("{e:#}"));
        }
        r
    }

    /// Browser sign-in for `client_id`; returns the token (also saved for the
    /// Web API when it is the client id search uses).
    fn authorize(&self, client_id: &str) -> Result<WebToken> {
        let client = librespot_oauth::OAuthClientBuilder::new(client_id, REDIRECT_URI, SCOPES.to_vec())
            .open_in_browser()
            .with_custom_message("Orpheus is now connected to Spotify. You can close this tab.")
            .build()
            .map_err(|e| anyhow!("{e}"))?;
        let token = client.get_access_token().map_err(|e| anyhow!("Spotify authorization failed: {e}"))?;
        let left = token.expires_at.saturating_duration_since(std::time::Instant::now());
        let t = WebToken {
            client_id: client_id.to_string(),
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: crate::model::now_ms() + left.as_millis() as i64,
        };
        if t.client_id == self.web_cid() {
            self.save_web_token(&t);
            *self.web_blocked_until.lock() = 0;
        }
        Ok(t)
    }

    /// Interactive sign-in for playback (opens the system browser).
    pub fn login(&self) -> Result<()> {
        self.status.lock().connecting = true;
        let r = (|| -> Result<()> {
            let t = self.authorize(DEFAULT_CLIENT_ID)?;
            self.rt.block_on(self.connect_with(Credentials::with_access_token(t.access_token), ""))
        })();
        if let Err(e) = &r {
            let mut st = self.status.lock();
            st.connecting = false;
            st.error = Some(format!("{e:#}"));
        }
        r
    }

    /// Deletes Spotify audio downloaded for playback (it's re-downloaded as needed).
    pub fn clear_cache(&self) -> Result<u64> {
        let dir = self.dir.join("audio");
        let size: u64 = walkdir::WalkDir::new(&dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(size)
    }

    pub fn cache_bytes(&self) -> u64 {
        walkdir::WalkDir::new(self.dir.join("audio"))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    }

    pub fn logout(&self) {
        if let Some(c) = self.conn.lock().take() {
            c.session.shutdown();
        }
        let _ = std::fs::remove_file(self.dir.join("credentials.json"));
        let _ = std::fs::remove_file(self.web_token_path());
        *self.web_token.lock() = None;
        *self.status.lock() = SpotifyStatus::default();
    }

    pub fn deck_player(&self, deck: usize) -> Result<Arc<DeckPlayer>> {
        self.ensure_connected()?;
        let mut conn = self.conn.lock();
        let c = conn.as_mut().ok_or_else(|| anyhow!("Spotify is not connected"))?;
        if c.players[deck].is_none() {
            c.players[deck] = Some(DeckPlayer::new(c.session.clone(), &self.rt));
        }
        Ok(c.players[deck].clone().unwrap())
    }

    pub fn track_metadata(&self, uri: &str) -> Result<SpotifyTrack> {
        let session = self.live_session()?;
        let id = SpotifyUri::from_uri(uri).map_err(|e| anyhow!("{e}"))?;
        self.rt.block_on(async { Self::fetch_track(&session, &id).await })
    }

    async fn fetch_track(session: &Session, id: &SpotifyUri) -> Result<SpotifyTrack> {
        let t = librespot_metadata::Track::get(session, id).await.map_err(|e| anyhow!("{e}"))?;
        Ok(SpotifyTrack {
            uri: id.to_uri().map_err(|e| anyhow!("{e}"))?,
            title: t.name.clone(),
            artist: t.artists.0.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", "),
            album: t.album.name.clone(),
            duration_ms: t.duration as i64,
            artwork_url: cover_url(&t.album.covers).or_else(|| cover_url(&t.album.cover_group)),
        })
    }

    /// Resolves a track / album / playlist URL or URI into its tracks.
    pub fn resolve(&self, input: &str) -> Result<Vec<SpotifyTrack>> {
        let session = self.live_session()?;
        let uri = normalize_uri(input).ok_or_else(|| anyhow!("Not a Spotify link: {input}"))?;
        let id = SpotifyUri::from_uri(&uri).map_err(|e| anyhow!("{e}"))?;
        self.rt.block_on(async move {
            let ids: Vec<SpotifyUri> = match &id {
                SpotifyUri::Track { .. } => vec![id.clone()],
                SpotifyUri::Album { .. } => {
                    let a = librespot_metadata::Album::get(&session, &id).await.map_err(|e| anyhow!("{e}"))?;
                    a.discs.0.iter().flat_map(|d| d.tracks.0.iter().cloned()).collect()
                }
                SpotifyUri::Playlist { .. } => {
                    let p = librespot_metadata::Playlist::get(&session, &id).await.map_err(|e| anyhow!("{e}"))?;
                    p.tracks().cloned().collect()
                }
                _ => return Err(anyhow!("Unsupported Spotify link (use a track, album or playlist)")),
            };
            let mut out = Vec::with_capacity(ids.len());
            // Small batches: bursts of metadata requests get rate-limited.
            for chunk in ids.chunks(4) {
                let futs = chunk
                    .iter()
                    .filter(|u| matches!(u, SpotifyUri::Track { .. }))
                    .map(|u| Self::fetch_track(&session, u));
                for r in futures_util::future::join_all(futs).await {
                    match r {
                        Ok(t) => out.push(t),
                        Err(e) => log::warn!("spotify track metadata: {e:#}"),
                    }
                }
            }
            Ok(out)
        })
    }

    /// Separate browser sign-in with your own Spotify app, for search and playlists.
    pub fn web_login(&self) -> Result<()> {
        let cid = self.web_cid();
        if cid == DEFAULT_CLIENT_ID {
            return Err(anyhow!("Enter your Spotify app's client ID first"));
        }
        self.authorize(&cid).map(|_| ())
    }

    /// A Web API access token from the browser sign-in, refreshed as needed.
    /// (librespot's internal "keymaster" tokens are refused for these scopes.)
    async fn web_token(&self) -> Result<String> {
        let own = self.web_cid() != DEFAULT_CLIENT_ID;
        let reauth = move || {
            if own {
                anyhow!("Spotify search and playlists need you to sign in with your app: Settings → Spotify → Sign In for Search.")
            } else {
                anyhow!("Spotify search and playlists need you to sign in once more: Settings → Spotify → Sign In Again.")
            }
        };
        let tok = self.web_token.lock().clone().filter(|t| t.client_id == self.web_cid()).ok_or_else(reauth)?;
        if tok.expires_at > crate::model::now_ms() + 60_000 {
            return Ok(tok.access_token);
        }
        if tok.refresh_token.is_empty() {
            return Err(reauth());
        }
        let client = librespot_oauth::OAuthClientBuilder::new(&tok.client_id, REDIRECT_URI, SCOPES.to_vec())
            .build()
            .map_err(|e| anyhow!("{e}"))?;
        let fresh = client.refresh_token_async(&tok.refresh_token).await.map_err(|e| {
            log::warn!("Spotify token refresh failed: {e}");
            reauth()
        })?;
        let left = fresh.expires_at.saturating_duration_since(std::time::Instant::now());
        let t = WebToken {
            client_id: tok.client_id,
            access_token: fresh.access_token.clone(),
            // Spotify may or may not rotate the refresh token.
            refresh_token: if fresh.refresh_token.is_empty() { tok.refresh_token } else { fresh.refresh_token },
            expires_at: crate::model::now_ms() + left.as_millis() as i64,
        };
        self.save_web_token(&t);
        Ok(fresh.access_token)
    }

    async fn web_get(&self, url: &str) -> Result<serde_json::Value> {
        let wait_ms = *self.web_blocked_until.lock() - crate::model::now_ms();
        if wait_ms > 0 {
            return Err(rate_limited(wait_ms));
        }
        let token = self.web_token().await?;
        let resp = self.http.get(url).bearer_auth(token).send().await.context("Spotify Web API")?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // Back off for as long as Spotify asks (default: 30 s) instead of retrying.
            let secs = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<i64>().ok())
                .unwrap_or(30)
                .clamp(1, 24 * 3600);
            *self.web_blocked_until.lock() = crate::model::now_ms() + secs * 1000;
            log::warn!("Spotify Web API rate limit: waiting {secs} s");
            return Err(rate_limited(secs * 1000));
        }
        let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
        if !status.is_success() {
            let msg = body["error"]["message"].as_str().unwrap_or("request failed");
            return Err(anyhow!("Spotify Web API {status}: {msg}"));
        }
        Ok(body)
    }

    /// GET with the largest page size Spotify accepts: `url` contains
    /// `{limit}`. The maximum differs between apps (and has changed over
    /// time), so on "Invalid limit" smaller sizes are tried and the one that
    /// works is remembered.
    async fn web_get_limited(&self, url: &str) -> Result<serde_json::Value> {
        loop {
            let limit = *self.page_limit.lock();
            match self.web_get(&url.replace("{limit}", &limit.to_string())).await {
                Err(e) if is_invalid_limit(&e) => {
                    let Some(&smaller) = PAGE_LIMITS.iter().find(|&&l| l < limit) else { return Err(e) };
                    log::info!("Spotify Web API: limit {limit} refused, using {smaller}");
                    *self.page_limit.lock() = smaller;
                }
                r => return r,
            }
        }
    }

    pub fn search(&self, query: &str) -> Result<Vec<SpotifyTrack>> {
        let key = query.trim().to_lowercase();
        let now = crate::model::now_ms();
        if let Some((at, hits)) = self.search_cache.lock().get(&key) {
            if now - at < SEARCH_CACHE_MS {
                return Ok(hits.clone());
            }
        }
        let url = format!(
            "https://api.spotify.com/v1/search?type=track&limit={{limit}}&q={}",
            urlencode(query.trim())
        );
        let v = self.rt.block_on(self.web_get_limited(&url))?;
        let hits: Vec<SpotifyTrack> =
            v["tracks"]["items"].as_array().map(|a| a.iter().filter_map(web_track).collect()).unwrap_or_default();
        let mut cache = self.search_cache.lock();
        cache.retain(|_, (at, _)| now - *at < SEARCH_CACHE_MS);
        if cache.len() >= SEARCH_CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, (now, hits.clone()));
        Ok(hits)
    }

    /// Spotify tracks that may be this audio: by ISRC if known, else by title
    /// and artist; only those within `max_len_diff_ms` of its length, closest
    /// first. (Whether one is the same audio takes a fingerprint.)
    pub fn candidates(&self, isrc: Option<&str>, title: &str, artist: &str, duration_ms: i64, max_len_diff_ms: i64) -> Result<Vec<SpotifyTrack>> {
        let mut hits = match isrc.map(str::trim).filter(|i| !i.is_empty()) {
            Some(i) => self.search(&format!("isrc:{i}"))?,
            None => vec![],
        };
        if hits.is_empty() {
            let q = format!("{} {}", title.trim(), artist.trim());
            if q.trim().is_empty() {
                return Ok(vec![]);
            }
            hits = self.search(&q)?;
        }
        Ok(closest_in_length(hits, duration_ms, max_len_diff_ms))
    }

    pub fn my_playlists(&self) -> Result<Vec<SpotifyPlaylist>> {
        self.rt.block_on(async {
            let mut out = vec![SpotifyPlaylist {
                uri: "liked".into(),
                name: "Liked Songs".into(),
                owner: String::new(),
                track_count: -1,
                artwork_url: None,
            }];
            let mut v = self.web_get_limited("https://api.spotify.com/v1/me/playlists?limit={limit}").await?;
            loop {
                for p in v["items"].as_array().cloned().unwrap_or_default() {
                    if p.is_null() {
                        continue;
                    }
                    out.push(SpotifyPlaylist {
                        uri: p["uri"].as_str().unwrap_or_default().to_string(),
                        name: p["name"].as_str().unwrap_or_default().to_string(),
                        owner: p["owner"]["display_name"].as_str().unwrap_or_default().to_string(),
                        track_count: p["tracks"]["total"].as_i64().or(p["items"]["total"].as_i64()).unwrap_or(0),
                        artwork_url: p["images"][0]["url"].as_str().map(String::from),
                    });
                }
                match v["next"].as_str() {
                    Some(next) => v = self.web_get(next).await?,
                    None => break,
                }
            }
            Ok(out)
        })
    }

    pub fn playlist_tracks(&self, uri: &str) -> Result<Vec<SpotifyTrack>> {
        if uri == "liked" {
            return self.rt.block_on(async {
                let mut out = vec![];
                let mut v = self.web_get_limited("https://api.spotify.com/v1/me/tracks?limit={limit}").await?;
                loop {
                    for it in v["items"].as_array().cloned().unwrap_or_default() {
                        if let Some(t) = web_track(&it["track"]) {
                            out.push(t);
                        }
                    }
                    match v["next"].as_str() {
                        Some(next) => v = self.web_get(next).await?,
                        None => break,
                    }
                }
                Ok(out)
            });
        }
        self.resolve(uri)
    }

    /// Decodes a Spotify track as fast as the network allows (not in real
    /// time) for BPM/waveform analysis. Audio is analysed in memory only.
    pub fn decode_for_analysis(&self, uri: &str) -> Result<(Vec<f32>, u32)> {
        let session = self.live_session()?;
        let id = SpotifyUri::from_uri(uri).map_err(|e| anyhow!("{e}"))?;
        let (tx, rx) = bounded::<Msg>(64);
        let gen = Arc::new(AtomicU64::new(1));
        let dp = {
            let _guard = self.rt.enter();
            let sink_gen = gen.clone();
            let sink_tx = tx.clone();
            let player = Player::new(
                PlayerConfig { bitrate: Bitrate::Bitrate160, gapless: false, ..Default::default() },
                session,
                Box::new(NoOpVolume),
                move || Box::new(ChannelSink { tx: sink_tx, gen: sink_gen }),
            );
            let mut events = player.get_player_event_channel();
            let ev_tx = tx.clone();
            self.rt.spawn(async move {
                while let Some(ev) = events.recv().await {
                    let m = match ev {
                        PlayerEvent::EndOfTrack { .. } => Msg::Eof(1),
                        PlayerEvent::Unavailable { .. } => Msg::Unavailable(1),
                        _ => continue,
                    };
                    let tx = ev_tx.clone();
                    let _ = tokio::task::spawn_blocking(move || tx.send_timeout(m, Duration::from_secs(30))).await;
                    break;
                }
            });
            player
        };
        drop(tx);
        dp.load(id, true, 0);
        let mut mono = vec![];
        loop {
            match rx.recv_timeout(Duration::from_secs(30)) {
                Ok(Msg::Audio(_, s)) => mono.extend(s.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5)),
                Ok(Msg::Eof(_)) => break,
                Ok(Msg::Unavailable(_)) => return Err(anyhow!("track unavailable")),
                Err(_) => return Err(anyhow!("timed out decoding Spotify track")),
            }
        }
        gen.fetch_add(1, Ordering::SeqCst);
        dp.stop();
        Ok((mono, SAMPLE_RATE))
    }
}

/// Tracks within `max_diff_ms` of a length, closest first, each once.
fn closest_in_length(mut hits: Vec<SpotifyTrack>, duration_ms: i64, max_diff_ms: i64) -> Vec<SpotifyTrack> {
    hits.retain(|t| (t.duration_ms - duration_ms).abs() <= max_diff_ms);
    hits.sort_by_key(|t| (t.duration_ms - duration_ms).abs());
    let mut seen = std::collections::HashSet::new();
    hits.retain(|t| seen.insert(t.uri.clone()));
    hits
}

/// Your own Spotify app's client id for search and playlists ("" = the shared one).
pub fn web_client_id(settings: &crate::settings::Settings) -> String {
    let id = settings.spotify.client_id.trim().to_string();
    if !id.is_empty() {
        return id;
    }
    std::env::var("ORPHEUS_SPOTIFY_CLIENT_ID").unwrap_or_default()
}

fn web_track(t: &serde_json::Value) -> Option<SpotifyTrack> {
    let uri = t["uri"].as_str()?;
    if !uri.starts_with("spotify:track:") {
        return None;
    }
    Some(SpotifyTrack {
        uri: uri.to_string(),
        title: t["name"].as_str().unwrap_or_default().to_string(),
        artist: t["artists"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default(),
        album: t["album"]["name"].as_str().unwrap_or_default().to_string(),
        duration_ms: t["duration_ms"].as_i64().unwrap_or(0),
        artwork_url: t["album"]["images"]
            .as_array()
            .and_then(|imgs| imgs.iter().find(|i| i["width"].as_i64().unwrap_or(0) <= 400).or(imgs.first()))
            .and_then(|i| i["url"].as_str())
            .map(String::from),
    })
}

fn rate_limited(wait_ms: i64) -> anyhow::Error {
    let secs = (wait_ms + 999) / 1000;
    let wait = if secs >= 120 { format!("{} min", (secs + 59) / 60) } else { format!("{secs} s") };
    anyhow!("Spotify is limiting requests; try again in {wait}. (Using your own Spotify app for search avoids the shared limit: Settings → Spotify.)")
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Accepts `spotify:track:…` URIs and open.spotify.com URLs.
pub fn normalize_uri(input: &str) -> Option<String> {
    let s = input.trim();
    if s.starts_with("spotify:") {
        return Some(s.to_string());
    }
    let rest = s
        .strip_prefix("https://open.spotify.com/")
        .or_else(|| s.strip_prefix("http://open.spotify.com/"))
        .or_else(|| s.strip_prefix("open.spotify.com/"))?;
    let rest = rest.split(['?', '#']).next()?;
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty() && !p.starts_with("intl-")).collect();
    match parts.as_slice() {
        [kind, id, ..] if ["track", "album", "playlist"].contains(kind) => Some(format!("spotify:{kind}:{id}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_by_length() {
        let t = |uri: &str, ms| SpotifyTrack {
            uri: uri.into(), title: String::new(), artist: String::new(), album: String::new(), duration_ms: ms, artwork_url: None,
        };
        let hits = vec![t("a", 200_000), t("b", 181_000), t("c", 179_500), t("b", 181_000), t("d", 184_000)];
        let uris: Vec<_> = closest_in_length(hits, 180_000, 3_000).into_iter().map(|t| t.uri).collect();
        assert_eq!(uris, ["c", "b"]);
    }

    #[test]
    fn normalizes_links() {
        assert_eq!(
            normalize_uri("https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC?si=abc").as_deref(),
            Some("spotify:track:4uLU6hMCjMI75M1A2tKUQC")
        );
        assert_eq!(
            normalize_uri("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M").as_deref(),
            Some("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M")
        );
        assert_eq!(normalize_uri("spotify:album:x").as_deref(), Some("spotify:album:x"));
        assert_eq!(normalize_uri("hello"), None);
    }
}
