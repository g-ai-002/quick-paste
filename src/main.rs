#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod brand;
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
mod ui;

fn main() {
    app::run();
}
