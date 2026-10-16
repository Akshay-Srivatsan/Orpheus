//! The engine as a QML object: `call` runs an engine command (by name, JSON
//! arguments) on a worker thread and answers with `replied`; the engine's
//! events arrive as `notified`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, failure)]
        type Engine = super::EngineRust;

        /// Starts a command; its answer comes with `replied` and this id.
        #[qinvokable]
        fn call(self: Pin<&mut Engine>, name: &QString, args: &QString) -> i32;

        /// A command's answer: its JSON result, or an error message.
        #[qsignal]
        fn replied(self: Pin<&mut Engine>, id: i32, ok: bool, result: &QString);

        /// An engine event: its name and JSON payload.
        #[qsignal]
        fn notified(self: Pin<&mut Engine>, name: &QString, payload: &QString);
    }

    impl cxx_qt::Threading for Engine {}
    impl cxx_qt::Initialize for Engine {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use orpheus_core::app::{App, EventSink};
use std::sync::Arc;

#[derive(Default)]
pub struct EngineRust {
    app: Option<Arc<App>>,
    next_id: i32,
    /// Why the engine couldn't start (shown instead of the window's contents).
    failure: QString,
}

/// Engine events, delivered on the Qt thread.
struct QtEvents(CxxQtThread<qobject::Engine>);

impl EventSink for QtEvents {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let (name, json) = (QString::from(event), QString::from(&payload.to_string()));
        // The window may already be gone while the engine shuts down.
        let _ = self.0.queue(move |o| o.notified(&name, &json));
    }
}

impl cxx_qt::Initialize for qobject::Engine {
    fn initialize(mut self: Pin<&mut Self>) {
        let started = orpheus_core::app::default_data_dir().and_then(|dir| {
            log::info!("data directory: {}", dir.display());
            App::new(Arc::new(QtEvents(self.qt_thread())), dir)
        });
        match started {
            Ok(app) => {
                app.launch();
                self.as_mut().rust_mut().app = Some(app);
            }
            Err(e) => {
                log::error!("Orpheus couldn't start: {e:#}");
                self.as_mut().set_failure(QString::from(&format!("Orpheus couldn't start: {e:#}")));
            }
        }
    }
}

impl qobject::Engine {
    fn call(mut self: Pin<&mut Self>, name: &QString, args: &QString) -> i32 {
        let id = {
            let mut r = self.as_mut().rust_mut();
            r.next_id = r.next_id.wrapping_add(1);
            r.next_id
        };
        let thread = self.qt_thread();
        let (name, args) = (name.to_string(), args.to_string());
        let Some(app) = self.rust().app.clone() else {
            let msg = QString::from("Orpheus isn't running");
            let _ = thread.queue(move |o| o.replied(id, false, &msg));
            return id;
        };
        // Commands can block (files, the network): never on the UI thread.
        std::thread::spawn(move || {
            let args = if args.trim().is_empty() { serde_json::Value::Null } else { serde_json::from_str(&args).unwrap_or(serde_json::Value::Null) };
            let (ok, text) = match orpheus_core::api::call(&app, &name, args) {
                Ok(v) => (true, v.to_string()),
                Err(e) => (false, e),
            };
            let text = QString::from(&text);
            let _ = thread.queue(move |o| o.replied(id, ok, &text));
        });
        id
    }
}

impl Drop for EngineRust {
    fn drop(&mut self) {
        if let Some(app) = &self.app {
            app.shutdown();
        }
    }
}
