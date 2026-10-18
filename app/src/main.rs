//! Orpheus: DJ software for social dancing (the Qt Quick interface).

mod engine;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

/// Qt's log categories kept quiet unless you ask for them (`QT_LOGGING_RULES`,
/// which comes after these, wins): Qt Wayland's text input warns, harmlessly,
/// each time focus leaves a text field ("Trying to disable … but 0x0 is focused").
const QUIET_QT_LOGS: &str = "qt.qpa.wayland.textinput.warning=false";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,librespot=warn,symphonia=warn")).init();
    let rules = match std::env::var("QT_LOGGING_RULES") {
        Ok(yours) if !yours.is_empty() => format!("{QUIET_QT_LOGS};{yours}"),
        _ => QUIET_QT_LOGS.to_string(),
    };
    // Before Qt starts, while nothing else runs (setting the environment isn't thread-safe).
    unsafe { std::env::set_var("QT_LOGGING_RULES", rules) };
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Orpheus/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
