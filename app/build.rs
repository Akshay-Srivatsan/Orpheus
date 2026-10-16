use cxx_qt_build::{CxxQtBuilder, QResource, QResources, QmlFile, QmlModule};
use std::path::Path;

/// QML files that are singletons: shared objects rather than components.
const SINGLETONS: &[&str] = &["Backend", "Session", "Theme"];

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
    // Icons for when the system's icon theme has none (`qrc:/icons/<name>.svg`).
    let mut icons = QResource::new().prefix("/");
    let mut names: Vec<_> = std::fs::read_dir("icons").expect("icons folder").flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "svg")).collect();
    names.sort();
    for path in names {
        println!("cargo::rerun-if-changed={}", path.display());
        icons = icons.file(path);
    }
    CxxQtBuilder::new_qml_module(QmlModule::new("Orpheus").qml_files(files))
        .qrc_resources(QResources::new().resource(icons))
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .files(["src/engine.rs"])
        .build();
}
