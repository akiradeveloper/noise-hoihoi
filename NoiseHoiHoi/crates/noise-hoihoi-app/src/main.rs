#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod signal_monitor;
#[cfg(windows)]
mod windows_app;

#[cfg(windows)]
fn main() -> eframe::Result {
    windows_app::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("NoiseHoiHoi currently supports Windows 11 x64");
}
