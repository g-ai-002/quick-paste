#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod hotkey;
mod memo;
mod memory_trim;
mod paste;
mod settings;
mod settings_window;
mod single_instance;
mod ui;

fn main() {
    app::run();
}
