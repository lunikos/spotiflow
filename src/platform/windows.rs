use std::path::{Path, PathBuf};

use super::{host_arch, run_quietly};
use crate::ver::Ver;

pub fn candidates() -> Vec<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join("Spotify"))
        .into_iter()
        .collect()
}

/// The version comes from xpui itself (see commands::install), since Spotify.exe has no plain-text one.
pub fn layout(root: &Path) -> (PathBuf, PathBuf, &'static str, Option<Ver>) {
    let arm = root.join("v8_context_snapshot.arm64.bin");
    let snapshot = if arm.is_file() {
        arm
    } else {
        root.join("v8_context_snapshot.bin")
    };
    (root.join("Spotify.exe"), snapshot, host_arch(), None)
}

pub fn quit() {
    run_quietly("taskkill", &["/F", "/IM", "Spotify.exe"]);
}
