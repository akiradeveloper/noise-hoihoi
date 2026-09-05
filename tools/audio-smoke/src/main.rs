#[cfg(windows)]
mod windows;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    windows::run()
}

#[cfg(not(windows))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("audio-smoke requires Windows 11 x64")
}
