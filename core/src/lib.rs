//! Orpheus's engine: DJ software for social dancing, without a user interface.
//! A UI drives it through `app::App` and hears from it through an `EventSink`.

pub mod analysis;
pub mod api;
pub mod app;
pub mod chime;
pub mod club;
pub mod fingerprint;
pub mod audio;
pub mod controls;
pub mod store;
pub mod decode;
pub mod library;
pub mod matching;
pub mod midi;
pub mod model;
pub mod perform;
pub mod setfile;
pub mod settings;
pub mod smart;
pub mod spotify;
pub mod stems;
pub mod tts;
pub mod views;
