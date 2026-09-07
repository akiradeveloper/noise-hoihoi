#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(any(windows, target_os = "linux"))]
mod app;
#[cfg(any(windows, target_os = "linux"))]
mod signal_monitor;

#[cfg(any(windows, target_os = "linux"))]
fn main() -> eframe::Result {
    app::run()
}

#[cfg(not(any(windows, target_os = "linux")))]
fn main() {
    eprintln!("NoiseHoiHoi currently supports Windows 11 x64 and Linux x86_64");
}
