use std::path::{Path, PathBuf};
use std::process::Command;

use super::{home, host_arch, run_quietly};
use crate::ver::Ver;

pub fn candidates() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = ["/opt/spotify", "/usr/share/spotify", "/usr/lib/spotify"]
        .iter()
        .map(PathBuf::from)
        .collect();
    dirs.push(home().join(".local/share/spotify-launcher/install/usr/share/spotify"));
    dirs
}

pub fn layout(root: &Path) -> (PathBuf, PathBuf, &'static str, Option<Ver>) {
    let bin = root.join("spotify");
    let version = Command::new(&bin)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .nth(2)
                .and_then(Ver::parse)
        });
    (
        bin,
        root.join("v8_context_snapshot.bin"),
        host_arch(),
        version,
    )
}

pub fn quit() {
    run_quietly("pkill", &["-x", "spotify"]);
}
