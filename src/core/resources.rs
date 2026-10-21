use std::path::PathBuf;

/// Works from a checkout, a portable release, or /usr/bin after package install.
pub fn data_dir() -> PathBuf {
    if let Some(directory) = std::env::var_os("NETEASE_SONGREC_DATA_DIR") {
        return PathBuf::from(directory);
    }

    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(bin_dir) = executable.parent()
    {
        candidates.push(bin_dir.join("assets"));
        if let Some(prefix) = bin_dir.parent() {
            candidates.push(prefix.join("share/netease-songrec-linux"));
        }
    }
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    candidates.push(checkout.clone());
    candidates
        .into_iter()
        .find(|directory| directory.join("netease/fingerprint_bridge.js").is_file())
        .unwrap_or(checkout)
}
