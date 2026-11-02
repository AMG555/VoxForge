// build.rs – embed Windows icon + version metadata into voxforge.exe
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Resolve absolute path to icon from CARGO_MANIFEST_DIR
        // (the crate root: crates/voxforge-cli/) → walk up two levels to workspace root
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set");
        let workspace_root = std::path::Path::new(&manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .expect("workspace root resolvable");
        let ico_path = workspace_root.join("assets").join("voxforge.ico");

        if !ico_path.exists() {
            eprintln!(
                "cargo:warning=voxforge.ico not found at {}, skipping icon embed",
                ico_path.display()
            );
            return;
        }

        // Ensure rc.exe from the Windows SDK is discoverable
        let sdk_rc_dirs = [
            r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64",
            r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64",
            r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64",
            r"C:\Program Files (x86)\Windows Kits\10\bin\x64",
        ];
        let current_path = std::env::var("PATH").unwrap_or_default();
        let extra: Vec<&str> = sdk_rc_dirs
            .iter()
            .copied()
            .filter(|d| std::path::Path::new(d).exists())
            .collect();
        if !extra.is_empty() {
            let new_path = format!("{};{}", extra.join(";"), current_path);
            // SAFETY: build scripts are single-threaded
            unsafe { std::env::set_var("PATH", &new_path) };
        }

        let mut res = winres::WindowsResource::new();
        // Use absolute path so rc.exe can find the file from any working dir
        res.set_icon(&ico_path.to_string_lossy());
        res.set("ProductName", "VoxForge");
        res.set("FileDescription", "VoxForge Neural Speech Studio");
        res.set("CompanyName", "VoxForge Team");
        res.set("LegalCopyright", "Copyright (c) 2025 VoxForge Contributors");
        res.set("OriginalFilename", "voxforge.exe");
        res.set("InternalName", "voxforge");
        res.set_version_info(winres::VersionInfo::PRODUCTVERSION, 0x0001_0000_0000_0000);
        res.set_version_info(winres::VersionInfo::FILEVERSION, 0x0001_0000_0000_0000);

        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=winres compile failed: {e}");
        }
    }
}
