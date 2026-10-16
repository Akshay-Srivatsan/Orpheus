use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
use std::path::Path;

/// QML files that are singletons: shared objects rather than components.
const SINGLETONS: &[&str] = &["Backend"];

fn main() {
    // Every .qml file in qml/ belongs to the Orpheus module.
    let mut files: Vec<QmlFile> = vec![];
    for entry in std::fs::read_dir("qml").expect("qml folder") {
        let path = entry.expect("qml entry").path();
        if path.extension().is_some_and(|e| e == "qml") {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let singleton = SINGLETONS.contains(&name.as_str());
            println!("cargo::rerun-if-changed={}", path.display());
            files.push(QmlFile::from(Path::new("qml").join(path.file_name().unwrap())).singleton(singleton));
        }
    }
    files.sort_by(|a, b| a.get_path().cmp(b.get_path()));
    CxxQtBuilder::new_qml_module(QmlModule::new("Orpheus").qml_files(files))
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .files(["src/engine.rs"])
        .build();
}
