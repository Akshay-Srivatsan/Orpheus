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

/// Actions a control can trigger. Kept here so the UI can list them.
pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("perform.playNext", "Play next song", "button"),
    ("perform.skipNext", "Skip the up-next song", "button"),
    ("perform.toggleAutoAdvance", "Toggle auto-advance", "button"),
    ("main.playPause", "Main: play / pause", "button"),
    ("main.fadeOut", "Main: fade out", "button"),
    ("main.stop", "Main: stop (only while paused)", "button"),
    ("main.tempo", "Main: tempo fader", "absolute"),
    ("main.tempoNudge", "Main: fine tempo (encoder)", "relative"),
    ("main.tempoReset", "Main: reset tempo to setlist value", "button"),
    ("main.trim", "Main: gain trim", "absolute"),
    ("main.pfl", "Main: headphone cue on/off", "button"),
    ("preview.playPause", "Preview: play / pause", "button"),
    ("preview.cue", "Preview: back to start", "button"),
    ("preview.seek", "Preview: scrub (jog wheel)", "relative"),
    ("preview.jump:0.25", "Preview: jump to 25%", "button"),
    ("preview.jump:0.5", "Preview: jump to 50%", "button"),
    ("preview.jump:0.75", "Preview: jump to 75%", "button"),
    ("preview.tempo", "Preview: tempo fader", "absolute"),
    ("preview.trim", "Preview: gain trim", "absolute"),
    ("master.volume", "Master volume", "absolute"),
    ("cue.volume", "Headphone volume", "absolute"),
    ("browse.scroll", "Library: move selection", "relative"),
    ("browse.loadPreview", "Library: preview selected song", "button"),
    ("browse.queueNext", "Library: play selected song next", "button"),
    ("browse.queueEnd", "Library: add selected song to end of set", "button"),
];

pub const LED_STATES: &[&str] = &[
    "main.playing",
    "main.fading",
    "main.pfl",
    "preview.playing",
    "perform.nextReady",
    "perform.autoAdvance",
];

pub fn builtin_mappings() -> Vec<Mapping> {
    let c = |status: u8, data1: u8, action: &str, kind: &str| Control {
        status,
        data1,
        action: action.into(),
        kind: kind.into(),
        invert: false,
    };
    let inv = |mut x: Control| {
        x.invert = true;
        x
    };
    let led = |status: u8, data1: u8, state: &str| Led { status, data1, state: state.into(), on: 0x7f, off: 0 };
    // Numark DJ2GO2 / DJ2GO2 Touch (MIDI layout from Mixxx's mapping).
    // Left deck = main ("on air"), right deck = preview (headphones only).
    // The crossfader is intentionally unmapped: songs never overlap.
    let dj2go2 = Mapping {
        id: "numark-dj2go2".into(),
        name: "Numark DJ2GO2 / DJ2GO2 Touch".into(),
        builtin: true,
        device_match: vec!["dj2go2".into()],
        init: vec![vec![0xF0, 0x00, 0x01, 0x3F, 0x3C, 0x48, 0xF7]],
        controls: vec![
            c(0x90, 0x00, "main.playPause", "button"),
            c(0x90, 0x01, "main.fadeOut", "button"),
            c(0x90, 0x02, "perform.playNext", "button"),
            c(0x90, 0x1B, "main.pfl", "button"),
            inv(c(0xB0, 0x09, "main.tempo", "absolute")),
            c(0xB0, 0x16, "main.trim", "absolute"),
            c(0x94, 0x01, "perform.playNext", "button"),
            c(0x94, 0x02, "main.fadeOut", "button"),
            c(0x94, 0x03, "perform.skipNext", "button"),
            c(0x94, 0x04, "perform.toggleAutoAdvance", "button"),
            c(0x9F, 0x02, "browse.queueNext", "button"),
            c(0x91, 0x00, "preview.playPause", "button"),
            c(0x91, 0x01, "preview.cue", "button"),
            c(0x91, 0x02, "browse.queueEnd", "button"),
            inv(c(0xB1, 0x09, "preview.tempo", "absolute")),
            c(0xB1, 0x16, "preview.trim", "absolute"),
            c(0xB1, 0x06, "preview.seek", "relative"),
            c(0x95, 0x01, "preview.cue", "button"),
            c(0x95, 0x02, "preview.jump:0.25", "button"),
            c(0x95, 0x03, "preview.jump:0.5", "button"),
            c(0x95, 0x04, "preview.jump:0.75", "button"),
            c(0x9F, 0x03, "browse.loadPreview", "button"),
            c(0xBF, 0x00, "browse.scroll", "relative"),
            c(0x9F, 0x06, "browse.loadPreview", "button"),
            c(0xBF, 0x0A, "master.volume", "absolute"),
            c(0xBF, 0x0C, "cue.volume", "absolute"),
        ],
        leds: vec![
            led(0x90, 0x00, "main.playing"),
            led(0x90, 0x01, "main.fading"),
            led(0x90, 0x02, "perform.nextReady"),
            led(0x90, 0x1B, "main.pfl"),
            led(0x91, 0x00, "preview.playing"),
            led(0x94, 0x01, "perform.nextReady"),
            led(0x94, 0x04, "perform.autoAdvance"),
        ],
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
}

pub struct Midi {
    state: Arc<Mutex<State>>,
    handler: Handler,
}

fn is_through_port(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("midi through") || n.contains("through port")
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

impl Midi {
    pub fn new(mapping: Mapping, enabled: bool, handler: Handler) -> Arc<Self> {
        let midi = Arc::new(Self {
            state: Arc::new(Mutex::new(State {
                mapping,
                inputs: HashMap::new(),
                outputs: HashMap::new(),
                led_state: HashMap::new(),
                learn: None,
                enabled,
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

    pub fn set_mapping(&self, mapping: Mapping, enabled: bool) {
        let mut st = self.state.lock();
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

    pub fn ports(&self) -> Vec<PortInfo> {
        let st = self.state.lock();
        let Ok(input) = MidiInput::new("orpheus-list") else { return vec![] };
        input
            .ports()
            .iter()
            .filter_map(|p| input.port_name(p).ok())
            .filter(|n| !is_through_port(n))
            .map(|name| PortInfo { connected: st.inputs.contains_key(&name), name })
            .collect()
    }

    fn refresh_ports(&self) -> bool {
        if !self.state.lock().enabled {
            return true;
        }
        let probe = match MidiInput::new("orpheus-probe") {
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
            .filter(|n| !is_through_port(n))
            .collect();
        drop(probe);
        // Drop vanished ports.
        {
            let mut st = self.state.lock();
            st.inputs.retain(|n, _| names.contains(n));
        }
        for name in &names {
            if self.state.lock().inputs.contains_key(name) {
                continue;
            }
            let Ok(input) = MidiInput::new("orpheus") else { continue };
            let Some(port) = input.ports().into_iter().find(|p| input.port_name(p).ok().as_deref() == Some(name)) else {
                continue;
            };
            let state = self.state.clone();
            let handler = self.handler.clone();
            match input.connect(&port, "orpheus-in", move |_ts, msg, _| on_message(&state, &handler, msg), ()) {
                Ok(conn) => {
                    log::info!("MIDI input connected: {name}");
                    self.state.lock().inputs.insert(name.clone(), conn);
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
        let Ok(out) = MidiOutput::new("orpheus") else { return };
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
    pub fn set_leds(&self, states: &HashMap<&str, bool>) {
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

fn on_message(state: &Arc<Mutex<State>>, handler: &Handler, msg: &[u8]) {
    if msg.len() < 3 {
        return;
    }
    let (status, d1, d2) = (msg[0], msg[1], msg[2]);
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
        }));
        on_message(&state, &handler, &[0x90, 0x00, 0x7f]); // main play press
        on_message(&state, &handler, &[0x80, 0x00, 0x00]); // release: ignored
        on_message(&state, &handler, &[0xB0, 0x09, 127]); // tempo fader (inverted)
        on_message(&state, &handler, &[0xBF, 0x00, 127]); // browse knob -1
        let got = got.lock();
        assert_eq!(got[0], ("main.playPause".into(), Value::Press));
        assert_eq!(got[1], ("main.tempo".into(), Value::Abs(0.0)));
        assert_eq!(got[2], ("browse.scroll".into(), Value::Rel(-1)));
        assert_eq!(got.len(), 3);
    }
}
