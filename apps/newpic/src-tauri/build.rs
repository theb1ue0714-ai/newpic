fn main() {
    // Tauri embeds a window icon on desktop targets even when bundling is
    // disabled. Keep the scaffold free of provisional brand artwork by
    // generating a neutral one-pixel build artifact until NewPic has an
    // approved icon set.
    const PLACEHOLDER_ICON: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc,
        0xcf, 0xc0, 0x50, 0x0f, 0x00, 0x05, 0x83, 0x02, 0x7f, 0x96, 0x69, 0x0f, 0xa9, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let icon_path = out_dir.join("newpic-placeholder-icon.png");
    std::fs::write(&icon_path, PLACEHOLDER_ICON).expect("write placeholder icon");

    // Windows resources require an ICO container. PNG payloads are valid ICO
    // entries, so wrap the same neutral pixel without introducing another
    // source asset.
    let mut windows_icon = vec![0, 0, 1, 0, 1, 0, 1, 1, 0, 0, 1, 0, 32, 0];
    windows_icon.extend_from_slice(&(PLACEHOLDER_ICON.len() as u32).to_le_bytes());
    windows_icon.extend_from_slice(&22_u32.to_le_bytes());
    windows_icon.extend_from_slice(PLACEHOLDER_ICON);
    let windows_icon_path = out_dir.join("newpic-placeholder-icon.ico");
    std::fs::write(&windows_icon_path, windows_icon).expect("write placeholder Windows icon");

    let escaped_icon_path = icon_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    println!("cargo:rustc-env=TAURI_CONFIG={{\"bundle\":{{\"icon\":[\"{escaped_icon_path}\"]}}}}");

    let attributes = tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().window_icon_path(windows_icon_path),
    );
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
