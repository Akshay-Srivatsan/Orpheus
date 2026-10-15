//! MIDI controller support: hot-plugged ports, JSON mappings, MIDI learn, LEDs.

use crossbeam_channel::{bounded, Sender};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    /// Full status byte including channel (e.g. 0x90 = note on, channel 1).
    /// Note-on and note-off for the same note are treated as one control.
    pub status: u8,
    pub data1: u8,
    pub action: String,
    /// `button`, `absolute` (faders/knobs) or `relative` (encoders/jog wheels).
    pub kind: String,
    #[serde(default)]
    pub invert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Led {
    pub status: u8,
    pub data1: u8,
    /// State name, e.g. `main.playing`.
    pub state: String,
    #[serde(default = "default_on")]
    pub on: u8,
    #[serde(default)]
    pub off: u8,
}

fn default_on() -> u8 {
    0x7f
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub builtin: bool,
    /// Case-insensitive substrings of port names this mapping is meant for.
    #[serde(default)]
    pub device_match: Vec<String>,
    /// Sysex messages sent on connect (e.g. ask the controller to report its knob positions).
    #[serde(default)]
    pub init: Vec<Vec<u8>>,
    pub controls: Vec<Control>,
    #[serde(default)]
    pub leds: Vec<Led>,
}

/// Actions a control can trigger: `(id, label, kind)`. Deck actions use the
/// prefix `deck0.` (Deck A) or `deck1.` (Deck B).
pub fn actions() -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> = vec![];
    let mut add = |id: &str, label: &str, kind: &str| v.push((id.into(), label.into(), kind.into()));
    add("perform.playNext", "Play next song", "button");
    add("perform.fadeOut", "Fade out the on-air deck", "button");
    add("perform.skipNext", "Skip the up-next song", "button");
    add("perform.toggleAutoAdvance", "Toggle auto-advance", "button");
    add("master.volume", "Master volume", "absolute");
    add("cue.volume", "Headphone volume", "absolute");
    add("cue.crossfader", "Crossfader (deck A ↔ deck B in the room)", "absolute");
    add("browse.scroll", "Library: move selection", "relative");
    add("browse.audition", "Library: load the selected song onto the free deck (paused)", "button");
    add("browse.queueNext", "Library: play selected song next", "button");
    add("browse.queueEnd", "Library: add selected song to end of set", "button");
    for (d, name) in [(0, "A"), (1, "B")] {
        let p = format!("deck{d}.");
        let mut dk = |id: &str, label: &str, kind: &str| {
            v.push((format!("{p}{id}"), format!("Deck {name}: {label}"), kind.into()))
        };
        dk("playPause", "play / pause", "button");
        dk("cue", "cue (CDJ style; hold to preview)", "hold");
        dk("goLive", "go live (put on air)", "button");
        dk("pfl", "headphone cue on/off", "button");
        dk("load", "load selected song", "button");
        dk("tempo", "tempo fader", "absolute");
        dk("tempoNudge", "fine tempo", "relative");
        dk("tempoReset", "reset tempo", "button");
        dk("trim", "gain", "absolute");
        dk("jog", "jog wheel (scrub)", "relative");
        dk("jogTouch", "jog wheel touch (fine scrub)", "hold");
        for n in 1..=4 {
            dk(&format!("hotcue{n}"), &format!("hot cue {n} (set / jump)"), "button");
            dk(&format!("hotcueClear{n}"), &format!("clear hot cue {n}"), "button");
        }
        for (s, l) in [("-15", "back 15 s"), ("-5", "back 5 s"), ("+5", "forward 5 s"), ("+15", "forward 15 s")] {
            dk(&format!("jump{s}"), &format!("jump {l}"), "button");
        }
        for (n, s) in crate::stems::STEMS.iter().enumerate() {
            dk(&format!("stem{}", n + 1), &format!("{s} stem on/off"), "button");
        }
        dk("setStart", "set the version's start here", "button");
        dk("setEnd", "set the version's end here", "button");
        dk("setFade", "set the version's fade-out here", "button");
        dk("clearPoints", "clear the version's start / end / fade", "button");
    }
    v
}

pub fn led_states() -> Vec<String> {
    let mut v: Vec<String> = vec!["perform.nextReady".into(), "perform.autoAdvance".into()];
    for d in 0..2 {
        for s in [
            "playing", "onAir", "sync", "pfl", "cueReady", "hotcue1", "hotcue2", "hotcue3", "hotcue4", "stem1", "stem2",
            "stem3", "stem4",
        ] {
            v.push(format!("deck{d}.{s}"));
        }
    }
    v
}

pub fn builtin_mappings() -> Vec<Mapping> {
    let c = |status: u8, data1: u8, action: &str, kind: &str| Control {
        status,
        data1,
        action: action.into(),
        kind: kind.into(),
        invert: false,
    };
    let led = |status: u8, data1: u8, state: &str| Led { status, data1, state: state.into(), on: 0x7f, off: 0 };
    // Numark DJ2GO2 / DJ2GO2 Touch (MIDI layout from Mixxx's mapping).
    let mut controls = vec![];
    let mut leds = vec![];
    for d in 0u8..2 {
        let p = |a: &str| format!("deck{d}.{a}");
        let (note, cc, pad) = (0x90 + d, 0xB0 + d, 0x94 + d);
        controls.push(c(note, 0x00, &p("playPause"), "button"));
        controls.push(c(note, 0x01, &p("cue"), "hold"));
        controls.push(c(note, 0x02, &p("goLive"), "button"));
        controls.push(c(note, 0x1B, &p("pfl"), "button"));
        controls.push(c(0x9F, 0x02 + d, &p("load"), "button"));
        let mut tempo = c(cc, 0x09, &p("tempo"), "absolute");
        tempo.invert = true;
        controls.push(tempo);
        controls.push(c(cc, 0x16, &p("trim"), "absolute"));
        controls.push(c(cc, 0x06, &p("jog"), "relative"));
        controls.push(c(note, 0x06, &p("jogTouch"), "hold"));
        for n in 1..=4u8 {
            // Cues mode; holding PAD MODE offsets the notes by 8 (clear).
            controls.push(c(pad, n, &p(&format!("hotcue{n}")), "button"));
            controls.push(c(pad, 0x08 + n, &p(&format!("hotcueClear{n}")), "button"));
            leds.push(led(pad, n, &p(&format!("hotcue{n}"))));
        }
        // Sampler mode: stems (vocals, drums, bass, other).
        for n in 1..=4u8 {
            controls.push(c(pad, 0x30 + n, &p(&format!("stem{n}")), "button"));
            leds.push(led(pad, 0x30 + n, &p(&format!("stem{n}"))));
        }
        leds.push(led(note, 0x00, &p("playing")));
        leds.push(led(note, 0x01, &p("cueReady")));
        leds.push(led(note, 0x02, &p("sync")));
        leds.push(led(note, 0x1B, &p("pfl")));
    }
    controls.push(c(0xBF, 0x00, "browse.scroll", "relative"));
    controls.push(c(0x9F, 0x06, "browse.audition", "button"));
    controls.push(c(0xBF, 0x0A, "master.volume", "absolute"));
    controls.push(c(0xBF, 0x0C, "cue.volume", "absolute"));
    // A normal crossfader (with soft takeover: Play Next moves it in software).
    controls.push(c(0xBF, 0x08, "cue.crossfader", "absolute"));
    let dj2go2 = Mapping {
        id: "numark-dj2go2".into(),
        name: "Numark DJ2GO2 / DJ2GO2 Touch".into(),
        builtin: true,
        device_match: vec!["dj2go2".into()],
        init: vec![vec![0xF0, 0x00, 0x01, 0x3F, 0x3C, 0x48, 0xF7]],
        controls,
        leds,
    };
    let generic = Mapping {
        id: "generic".into(),
        name: "Generic (use MIDI Learn)".into(),
        builtin: true,
        device_match: vec![],
        init: vec![],
        controls: vec![],
        leds: vec![],
    };
    vec![dj2go2, generic]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// Button press.
    Press,
    /// Button release (only for `hold` controls).
    Release,
    /// Absolute position 0..=1.
    Abs(f64),
    /// Relative ticks (positive = clockwise).
    Rel(i32),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    pub name: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedMessage {
    pub status: u8,
    pub data1: u8,
    pub kind: String,
}

pub type Handler = Arc<dyn Fn(&str, Value) + Send + Sync>;

struct State {
    mapping: Mapping,
    inputs: HashMap<String, MidiInputConnection<()>>,
    outputs: HashMap<String, MidiOutputConnection>,
    led_state: HashMap<String, bool>,
    learn: Option<Sender<LearnedMessage>>,
    enabled: bool,
    /// Last message and the port it came from, to drop copies of the same
    /// event arriving through a second port (e.g. PipeWire's Midi-Bridge).
    last: Option<([u8; 3], std::time::Instant, String)>,
    /// Recently sent LED messages: if one comes back in (a MIDI loopback),
    /// it must not be treated as a button press.
    sent: std::collections::VecDeque<([u8; 3], std::time::Instant)>,
    /// Automatic mode: mappings to choose from by the connected port's name.
    auto: Option<Vec<Mapping>>,
}

pub struct Midi {
    state: Arc<Mutex<State>>,
    handler: Handler,
}

/// Client name Orpheus registers its MIDI ports under.
const CLIENT: &str = "orpheus";

/// Ports that must never be opened as inputs: system loopbacks, and Orpheus'
/// own output port, which ALSA lists among the inputs. Listening to it would
/// feed every LED message back in as a button press.
fn is_ignored_port(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("midi through") || n.contains("through port") || n.starts_with(&format!("{CLIENT}:")) || n.starts_with(&format!("{CLIENT}-"))
}

/// Identifies the physical device behind a port name, so the same controller
/// exposed twice (directly and via PipeWire's "Midi-Bridge") connects once.
fn device_key(name: &str) -> String {
    let mut n = base_name(name);
    if let Some(rest) = n.strip_prefix("midi-bridge:") {
        n = rest.to_string();
    }
    let last = n.rsplit(':').next().unwrap_or(&n).to_string();
    let mut out = String::new();
    let mut depth = 0;
    for ch in last.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_bridge(name: &str) -> bool {
    name.to_lowercase().starts_with("midi-bridge")
}

fn base_name(name: &str) -> String {
    // ALSA: "DJ2GO2 Touch:DJ2GO2 Touch MIDI 1 20:0"; CoreMIDI: "DJ2GO2 Touch".
    // Strip trailing client:port numbers and in/out suffixes so pairs match.
    let mut n = name.to_lowercase();
    if let Some(i) = n.rfind(' ') {
        let tail = &n[i + 1..];
        if tail.chars().all(|c| c.is_ascii_digit() || c == ':') {
            n.truncate(i);
        }
    }
    for suf in [" in", " out", " input", " output"] {
        if let Some(s) = n.strip_suffix(suf) {
            n = s.to_string();
        }
    }
    n
}

impl State {
    fn auto_pick(&self) -> Option<Mapping> {
        let candidates = self.auto.as_ref()?;
        self.inputs.keys().find_map(|port| {
            let p = port.to_lowercase();
            candidates.iter().find(|m| m.device_match.iter().any(|d| !d.is_empty() && p.contains(&d.to_lowercase()))).cloned()
        })
    }
}

impl Midi {
    pub fn new(mapping: Mapping, enabled: bool, auto: Option<Vec<Mapping>>, handler: Handler) -> Arc<Self> {
        let midi = Arc::new(Self {
            state: Arc::new(Mutex::new(State {
                mapping,
                inputs: HashMap::new(),
                outputs: HashMap::new(),
                led_state: HashMap::new(),
                learn: None,
                enabled,
                last: None,
                sent: Default::default(),
                auto,
            })),
            handler,
        });
        let m = midi.clone();
        std::thread::Builder::new()
            .name("midi-hotplug".into())
            .spawn(move || loop {
                // Back off when MIDI is unavailable (e.g. no access to the ALSA sequencer).
                let ok = m.refresh_ports();
                std::thread::sleep(Duration::from_secs(if ok { 2 } else { 30 }));
            })
            .expect("spawn midi thread");
        midi
    }

    /// Sets the active mapping. With `auto`, the mapping whose `device_match`
    /// fits a connected controller is picked from those candidates instead.
    pub fn set_mapping(&self, mapping: Mapping, enabled: bool, auto: Option<Vec<Mapping>>) {
        let mut st = self.state.lock();
        st.auto = auto;
        let mapping = st.auto_pick().unwrap_or(mapping);
        let reinit = st.mapping.id != mapping.id;
        st.mapping = mapping;
        st.enabled = enabled;
        st.led_state.clear();
        if !enabled {
            st.inputs.clear();
            st.outputs.clear();
        } else if reinit {
            let init = st.mapping.init.clone();
            for out in st.outputs.values_mut() {
                for msg in &init {
                    let _ = out.send(msg);
                }
            }
        }
    }

    /// Id of the mapping in use (resolved, also in automatic mode).
    pub fn active_mapping(&self) -> String {
        self.state.lock().mapping.id.clone()
    }

    pub fn ports(&self) -> Vec<PortInfo> {
        let st = self.state.lock();
        let Ok(input) = MidiInput::new(CLIENT) else { return vec![] };
        input
            .ports()
            .iter()
            .filter_map(|p| input.port_name(p).ok())
            .filter(|n| !is_ignored_port(n))
            .map(|name| PortInfo { connected: st.inputs.contains_key(&name), name })
            .collect()
    }

    fn refresh_ports(&self) -> bool {
        if !self.state.lock().enabled {
            return true;
        }
        let probe = match MidiInput::new(CLIENT) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("MIDI unavailable: {e}");
                return false;
            }
        };
        let names: Vec<String> = probe
            .ports()
            .iter()
            .filter_map(|p| probe.port_name(p).ok())
            .filter(|n| !is_ignored_port(n))
            .collect();
        drop(probe);
        // One port per physical device; prefer direct ports over bridged copies.
        let mut chosen: HashMap<String, String> = HashMap::new();
        for n in &names {
            let key = device_key(n);
            match chosen.get(&key) {
                Some(existing) if !is_bridge(existing) => {}
                Some(_) if is_bridge(n) => {}
                _ => {
                    chosen.insert(key, n.clone());
                }
            }
        }
        let names: Vec<String> = names.into_iter().filter(|n| chosen.values().any(|c| c == n)).collect();
        // Drop vanished ports.
        {
            let mut st = self.state.lock();
            st.inputs.retain(|n, _| names.contains(n));
        }
        for name in &names {
            if self.state.lock().inputs.contains_key(name) {
                continue;
            }
            let Ok(input) = MidiInput::new(CLIENT) else { continue };
            let Some(port) = input.ports().into_iter().find(|p| input.port_name(p).ok().as_deref() == Some(name)) else {
                continue;
            };
            let state = self.state.clone();
            let handler = self.handler.clone();
            let port_name = name.clone();
            match input.connect(&port, "orpheus-in", move |_ts, msg, _| on_message(&state, &handler, msg, &port_name), ()) {
                Ok(conn) => {
                    log::info!("MIDI input connected: {name}");
                    {
                        let mut st = self.state.lock();
                        st.inputs.insert(name.clone(), conn);
                        if let Some(m) = st.auto_pick().filter(|m| m.id != st.mapping.id) {
                            log::info!("using MIDI mapping \"{}\" for {name}", m.name);
                            st.mapping = m;
                            st.led_state.clear();
                        }
                    }
                    self.connect_output(name);
                }
                Err(e) => log::warn!("MIDI connect {name}: {e}"),
            }
        }
        let mut st = self.state.lock();
        let inputs: Vec<String> = st.inputs.keys().cloned().collect();
        st.outputs.retain(|n, _| inputs.contains(n));
        true
    }

    fn connect_output(&self, input_name: &str) {
        let Ok(out) = MidiOutput::new(CLIENT) else { return };
        let want = base_name(input_name);
        let Some(port) = out.ports().into_iter().find(|p| out.port_name(p).map(|n| base_name(&n) == want).unwrap_or(false)) else {
            return;
        };
        match out.connect(&port, "orpheus-out") {
            Ok(mut conn) => {
                let mut st = self.state.lock();
                for msg in st.mapping.init.clone() {
                    let _ = conn.send(&msg);
                }
                st.led_state.clear();
                st.outputs.insert(input_name.to_string(), conn);
            }
            Err(e) => log::warn!("MIDI output {input_name}: {e}"),
        }
    }

    /// Updates controller LEDs; only changed states are sent.
    pub fn set_leds(&self, states: &HashMap<String, bool>) {
        let mut st = self.state.lock();
        if st.outputs.is_empty() {
            return;
        }
        let leds = st.mapping.leds.clone();
        let mut msgs = vec![];
        for led in &leds {
            let on = states.get(led.state.as_str()).copied().unwrap_or(false);
            let key = format!("{}:{}", led.status, led.data1);
            if st.led_state.get(&key) != Some(&on) {
                st.led_state.insert(key, on);
                msgs.push([led.status, led.data1, if on { led.on } else { led.off }]);
            }
        }
        let now = std::time::Instant::now();
        for m in &msgs {
            st.sent.push_back((*m, now));
        }
        while st.sent.len() > 64 {
            st.sent.pop_front();
        }
        for out in st.outputs.values_mut() {
            for m in &msgs {
                let _ = out.send(m);
            }
        }
    }

    /// Waits for the next incoming control message (for MIDI learn).
    pub fn learn(&self, timeout: Duration) -> Option<LearnedMessage> {
        let (tx, rx) = bounded(1);
        self.state.lock().learn = Some(tx);
        let r = rx.recv_timeout(timeout).ok();
        self.state.lock().learn = None;
        r
    }

    pub fn cancel_learn(&self) {
        self.state.lock().learn = None;
    }
}

fn on_message(state: &Arc<Mutex<State>>, handler: &Handler, msg: &[u8], port: &str) {
    if msg.len() < 3 {
        return;
    }
    let (status, d1, d2) = (msg[0], msg[1], msg[2]);
    {
        let mut st = state.lock();
        let now = std::time::Instant::now();
        let bytes = [status, d1, d2];
        // Our own LED message coming back in: ignore it.
        st.sent.retain(|(_, t)| now.duration_since(*t) < Duration::from_millis(250));
        if let Some(i) = st.sent.iter().position(|(b, _)| *b == bytes) {
            st.sent.remove(i);
            return;
        }
        if let Some((b, t, p)) = &st.last {
            if *b == bytes && p != port && now.duration_since(*t) < Duration::from_millis(8) {
                return; // the same event delivered through another port
            }
        }
        st.last = Some((bytes, now, port.to_string()));
    }
    let kind = status & 0xF0;
    let ch = status & 0x0F;
    if kind != 0x80 && kind != 0x90 && kind != 0xB0 {
        return;
    }
    let is_press = kind == 0x90 && d2 > 0;
    let is_release = kind == 0x80 || (kind == 0x90 && d2 == 0);
    let action = {
        let mut st = state.lock();
        if let Some(tx) = st.learn.take() {
            if is_release {
                st.learn = Some(tx); // wait for a press or CC
                return;
            }
            let learned = LearnedMessage {
                status: if kind == 0x80 { 0x90 | ch } else { status },
                data1: d1,
                kind: if kind == 0xB0 { "absolute" } else { "button" }.into(),
            };
            let _ = tx.send(learned);
            return;
        }
        let note_status = if kind == 0x80 { 0x90 | ch } else { status };
        st.mapping
            .controls
            .iter()
            .find(|c| (c.status == note_status || c.status == status) && c.data1 == d1)
            .cloned()
    };
    let Some(c) = action else { return };
    let value = match c.kind.as_str() {
        "button" => {
            if kind == 0xB0 {
                if d2 >= 64 { Value::Press } else { return }
            } else if is_press {
                Value::Press
            } else {
                return;
            }
        }
        "hold" => {
            if kind == 0xB0 {
                if d2 >= 64 { Value::Press } else { Value::Release }
            } else if is_press {
                Value::Press
            } else {
                Value::Release
            }
        }
        "relative" => {
            let v = if d2 >= 64 { d2 as i32 - 128 } else { d2 as i32 };
            if v == 0 {
                return;
            }
            Value::Rel(if c.invert { -v } else { v })
        }
        _ => {
            let v = d2 as f64 / 127.0;
            Value::Abs(if c.invert { 1.0 - v } else { v })
        }
    };
    handler(&c.action, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_names_match_pairs() {
        assert_eq!(base_name("DJ2GO2 Touch:DJ2GO2 Touch MIDI 1 20:0"), base_name("DJ2GO2 Touch:DJ2GO2 Touch MIDI 1 20:0"));
        assert_eq!(base_name("DJ2GO2 Touch In"), base_name("DJ2GO2 Touch Out"));
    }

    #[test]
    fn own_ports_are_never_inputs() {
        assert!(is_ignored_port("orpheus:orpheus-out 130:0"));
        assert!(is_ignored_port("Midi Through:Midi Through Port-0 14:0"));
        assert!(!is_ignored_port("DJ2GO2 Touch:DJ2GO2 Touch MIDI 1 24:0"));
    }

    #[test]
    fn echoed_led_messages_are_ignored() {
        let got: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let g = got.clone();
        let handler: Handler = Arc::new(move |a, v| g.lock().push((a.to_string(), v)));
        let state = Arc::new(Mutex::new(State {
            mapping: builtin_mappings().remove(0),
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            led_state: HashMap::new(),
            learn: None,
            enabled: true,
            last: None,
            sent: Default::default(),
            auto: None,
        }));
        // The PLAY LED was just lit; the same bytes arriving must not press PLAY.
        state.lock().sent.push_back(([0x90, 0x00, 0x7f], std::time::Instant::now()));
        on_message(&state, &handler, &[0x90, 0x00, 0x7f], "loop");
        assert!(got.lock().is_empty());
        // A real press afterwards still works.
        on_message(&state, &handler, &[0x90, 0x00, 0x7f], "dj2go2");
        assert_eq!(got.lock().len(), 1);
    }

    #[test]
    fn bridged_ports_are_the_same_device() {
        let direct = "DJ2GO2 Touch:DJ2GO2 Touch MIDI 1 24:0";
        let bridged = "Midi-Bridge:DJ2GO2 Touch:(capture_0) DJ2GO2 Touch MIDI 1 14:1";
        assert_eq!(device_key(direct), device_key(bridged));
        assert_ne!(device_key(direct), device_key("Launchpad:Launchpad MIDI 1 28:0"));
    }

    #[test]
    fn dj2go2_mapping_has_no_duplicate_messages() {
        let m = builtin_mappings().remove(0);
        let mut seen = std::collections::HashSet::new();
        for c in &m.controls {
            assert!(seen.insert((c.status, c.data1)), "duplicate {:02X} {:02X}", c.status, c.data1);
        }
        let ids: std::collections::HashSet<String> = actions().into_iter().map(|a| a.0).collect();
        for c in &m.controls {
            assert!(ids.contains(&c.action), "unknown action {}", c.action);
        }
    }

    #[test]
    fn dispatches_mapped_controls() {
        let got: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let g = got.clone();
        let handler: Handler = Arc::new(move |a, v| g.lock().push((a.to_string(), v)));
        let state = Arc::new(Mutex::new(State {
            mapping: builtin_mappings().remove(0),
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            led_state: HashMap::new(),
            learn: None,
            enabled: true,
            last: None,
            sent: Default::default(),
            auto: None,
        }));
        on_message(&state, &handler, &[0x90, 0x00, 0x7f], "a"); // deck A play press
        on_message(&state, &handler, &[0x90, 0x00, 0x7f], "bridge"); // same event via a 2nd port: dropped
        on_message(&state, &handler, &[0x80, 0x00, 0x00], "a"); // release: ignored for buttons
        on_message(&state, &handler, &[0xB1, 0x09, 127], "a"); // deck B tempo fader (inverted)
        on_message(&state, &handler, &[0xBF, 0x00, 127], "a"); // browse knob -1
        on_message(&state, &handler, &[0x91, 0x01, 0x7f], "a"); // deck B cue press
        on_message(&state, &handler, &[0x91, 0x01, 0x00], "a"); // ... and release
        on_message(&state, &handler, &[0xB0, 0x06, 0x7f], "a"); // jog -1
        on_message(&state, &handler, &[0xB0, 0x06, 0x7f], "a"); // repeated tick on the same port is kept
        let got = got.lock();
        let want: Vec<(String, Value)> = vec![
            ("deck0.playPause".into(), Value::Press),
            ("deck1.tempo".into(), Value::Abs(0.0)),
            ("browse.scroll".into(), Value::Rel(-1)),
            ("deck1.cue".into(), Value::Press),
            ("deck1.cue".into(), Value::Release),
            ("deck0.jog".into(), Value::Rel(-1)),
            ("deck0.jog".into(), Value::Rel(-1)),
        ];
        assert_eq!(*got, want);
    }
}
