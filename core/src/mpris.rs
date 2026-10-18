//! A media player on the session bus (MPRIS 2, Linux). Desktops send media
//! keys (play/pause, next, stop) to the player that's playing, so they come
//! here instead of starting another player. Settings → General → Media keys:
//! taken and ignored (the default), controlling Orpheus, or not registered.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const BUS_NAME: &str = "org.mpris.MediaPlayer2.orpheus";
const PATH: &str = "/org/mpris/MediaPlayer2";
/// The track id MPRIS asks for (one at a time: the song the room hears).
const TRACK_PATH: &str = "/org/orpheus/MediaPlayer2/CurrentTrack";

/// A media key's command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
}

/// What the player shows: whether the room hears a song, and which.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub playing: bool,
    /// A song is loaded on the deck the room hears (paused, rather than stopped).
    pub loaded: bool,
    pub title: String,
    pub artist: String,
    pub length_ms: i64,
}

type OnCommand = Arc<dyn Fn(Command) + Send + Sync>;

/// The running player: on the bus as long as it lives.
pub struct Mpris {
    state: watch::Sender<State>,
    control: Arc<AtomicBool>,
    _task: tokio::task::JoinHandle<()>,
}

impl Mpris {
    /// Puts the player on the session bus (in `rt`); media keys call
    /// `on_command` when `control` is on, else they're taken and ignored.
    pub fn start(rt: &tokio::runtime::Handle, control: bool, on_command: impl Fn(Command) + Send + Sync + 'static) -> Self {
        let (tx, mut rx) = watch::channel(State::default());
        let control = Arc::new(AtomicBool::new(control));
        let shared = Arc::new(Mutex::new(State::default()));
        let player = Player { state: shared.clone(), control: control.clone(), on: Arc::new(on_command) };
        let task = rt.spawn(async move {
            let conn = match zbus::connection::Builder::session()
                .and_then(|b| b.name(BUS_NAME))
                .and_then(|b| b.serve_at(PATH, Root))
                .and_then(|b| b.serve_at(PATH, player))
            {
                Ok(b) => match b.build().await {
                    Ok(c) => c,
                    Err(e) => return log::warn!("media keys (MPRIS): {e}"),
                },
                Err(e) => return log::warn!("media keys (MPRIS): {e}"),
            };
            log::info!("media keys: on the session bus as {BUS_NAME}");
            // Tell the desktop when what's playing changes.
            while rx.changed().await.is_ok() {
                let next = rx.borrow_and_update().clone();
                *shared.lock() = next;
                if let Ok(iface) = conn.object_server().interface::<_, Player>(PATH).await {
                    let ctx = iface.signal_context();
                    let p = iface.get().await;
                    let _ = p.playback_status_changed(ctx).await;
                    let _ = p.metadata_changed(ctx).await;
                }
            }
        });
        Self { state: tx, control, _task: task }
    }

    /// What's playing now (sent on only when it changed).
    pub fn update(&self, s: State) {
        self.state.send_if_modified(|old| {
            let changed = *old != s;
            *old = s;
            changed
        });
    }

    pub fn set_control(&self, on: bool) {
        self.control.store(on, Ordering::Relaxed);
    }
}

impl Drop for Mpris {
    /// Off the bus (its connection closes with the task).
    fn drop(&mut self) {
        self._task.abort();
    }
}

struct Root;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        "Orpheus".into()
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "orpheus".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec![]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        vec![]
    }
}

struct Player {
    state: Arc<Mutex<State>>,
    control: Arc<AtomicBool>,
    on: OnCommand,
}

impl Player {
    fn command(&self, c: Command) {
        if self.control.load(Ordering::Relaxed) {
            (self.on)(c);
        } else {
            log::info!("media key {c:?} taken and ignored (Settings → General → Media keys)");
        }
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.command(Command::Next);
    }
    fn previous(&self) {}
    fn pause(&self) {
        self.command(Command::Pause);
    }
    fn play_pause(&self) {
        self.command(Command::PlayPause);
    }
    fn stop(&self) {
        self.command(Command::Stop);
    }
    fn play(&self) {
        self.command(Command::Play);
    }
    fn seek(&self, _offset: i64) {}
    fn set_position(&self, _track: ObjectPath<'_>, _position: i64) {}
    fn open_uri(&self, _uri: String) {}

    #[zbus(property)]
    fn playback_status(&self) -> String {
        let s = self.state.lock();
        if s.playing { "Playing" } else if s.loaded { "Paused" } else { "Stopped" }.into()
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let s = self.state.lock().clone();
        let mut m = HashMap::new();
        let mut put = |k: &str, v: Value<'_>| {
            if let Ok(v) = v.try_to_owned() {
                m.insert(k.to_string(), v);
            }
        };
        if let Ok(p) = ObjectPath::try_from(TRACK_PATH) {
            put("mpris:trackid", Value::from(p));
        }
        if !s.title.is_empty() {
            put("xesam:title", Value::from(s.title.as_str()));
        }
        if !s.artist.is_empty() {
            put("xesam:artist", Value::from(vec![s.artist.as_str()]));
        }
        if s.length_ms > 0 {
            put("mpris:length", Value::from(s.length_ms * 1000));
        }
        m
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        0
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }
    // Media keys only reach a player that can be controlled (even one that ignores them).
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}
