#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod memo;
mod paste;
mod single_instance;
mod ui;

fn main() {
    app::run();
}
