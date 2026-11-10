//! screenSleep: turns the screen off after a period of inactivity, showing a
//! cancellable countdown first. Lives in the system tray.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod config;
mod hotkey;
mod lang;
#[cfg(windows)]
mod media;
mod policy;
mod schema;
#[cfg(windows)]
mod settings;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("screenSleep only runs on Windows.");
    std::process::exit(1);
}
