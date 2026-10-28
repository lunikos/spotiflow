use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{home, host_arch, run_quietly};
use crate::ver::Ver;
use crate::Result;

pub fn candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications/Spotify.app"),
        home().join("Applications/Spotify.app"),
    ]
}

pub fn layout(root: &Path) -> (PathBuf, PathBuf, &'static str, Option<Ver>) {
    let bin = root.join("Contents/MacOS/Spotify");
    let arch = mach_arch(&bin).unwrap_or(host_arch());
    let snapshot = root
        .join("Contents/Frameworks/Chromium Embedded Framework.framework/Resources")
        .join(format!("v8_context_snapshot.{arch}.bin"));
    (bin, snapshot, arch, version(root))
}

pub fn quit() {
    run_quietly("osascript", &["-e", "quit app \"Spotify\""]);
    run_quietly("pkill", &["-x", "Spotify"]);
}

// macOS seals bundle resources, so any edit needs an ad hoc re-sign.

pub fn resign(root: &Path) -> Result<()> {
    let ok = Command::new("codesign")
        .args(["-f", "--deep", "-s", "-"])
        .arg(root)
        .output()?
        .status
        .success();
    if ok {
        Ok(())
    } else {
        Err("codesign failed (install Xcode Command Line Tools: xcode-select --install)".into())
    }
}

fn version(root: &Path) -> Option<Ver> {
    let out = Command::new("plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw"])
        .arg(root.join("Contents/Info.plist"))
        .output()
        .ok()?;
    Ver::parse(&String::from_utf8_lossy(&out.stdout))
}

fn mach_arch(bin: &Path) -> Option<&'static str> {
    let mut head = [0u8; 8];
    std::io::Read::read_exact(&mut fs::File::open(bin).ok()?, &mut head).ok()?;
    if head[..4] != [0xcf, 0xfa, 0xed, 0xfe] {
        return None;
    }
    match u32::from_le_bytes([head[4], head[5], head[6], head[7]]) {
        0x0100_000c => Some("arm64"),
        0x0100_0007 => Some("x86_64"),
        _ => None,
    }
}
