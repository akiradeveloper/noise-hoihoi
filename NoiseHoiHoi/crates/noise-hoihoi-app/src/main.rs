#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_app;

#[cfg(windows)]
fn main() -> eframe::Result {
    windows_app::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("NoiseHoiHoi v0.1 only supports Windows 11 x64");
}
