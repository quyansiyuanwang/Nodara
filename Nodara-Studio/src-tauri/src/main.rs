// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Desktop shell for the Studio.
//!
//! The shell deliberately contains no editor logic. It loads the same bundle
//! the browser build serves, and the app talks to the runtime over HTTP exactly
//! as it does in a tab — so there is one implementation to reason about.

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running the Studio shell");
}
