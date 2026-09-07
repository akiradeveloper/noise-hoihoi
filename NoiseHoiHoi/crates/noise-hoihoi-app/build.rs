fn main() -> std::io::Result<()> {
    const ICON: &str = "../../../assets/NoiseHoiHoi.ico";
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");

    // Build scripts run on the host, including when cross-compiling from Linux.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(ICON)
            .set("ProductName", "NoiseHoiHoi")
            .compile()?;
    }
    Ok(())
}
