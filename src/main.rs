#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod brand;
mod elevation;
mod hotkey;
mod logging;
mod memo;
mod memory_trim;
mod paste;
mod preview;
mod settings;
mod settings_window;
mod single_instance;
mod source;
mod themes;
mod tray;
mod ui;

fn main() {
    app::run();
}
