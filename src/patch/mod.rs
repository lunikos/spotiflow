//! Patch table from SpotX-Bash (see `upstream`), one record per line:
//! `group&name&regex&replacement&flags&file&min&max&platform&arch&count`

mod translate;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::platform::{Client, Os};
use crate::ver::Ver;
use crate::Result;

pub struct Patch {
    pub group: &'static str,
    pub name: &'static str,
    pub regex: &'static str,
    pub replace: &'static str,
    pub flags: &'static str,
    pub file: &'static str,
    min: Option<Ver>,
    max: Option<Ver>,
    platform: &'static str,
    arch: &'static str,
}

const ABOUT: &str = concat!("$1\"", include_str!("../../assets/about.html"), "\"");

/// Upstream's About-dialog patch keeps its pattern but shows spotiflow's text.
pub fn table() -> Vec<Patch> {
    let mut patches = parse(crate::upstream::data().patches);
    for p in patches.iter_mut().filter(|p| p.name == "aboutSpotX") {
        p.replace = ABOUT;
    }
    patches
}

/// Fails if any record won't compile, so a bad upstream update is never cached.
pub fn validate(table: &'static str) -> Result<usize> {
    let patches = parse(table);
    for p in &patches {
        translate::compile(p.regex, p.replace, p.flags)
            .map_err(|e| format!("patch {}: {e}", p.name))?;
    }
    Ok(patches.len())
}

fn parse(table: &'static str) -> Vec<Patch> {
    table
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let f: Vec<&'static str> = l.split('&').collect();
            let at = |i: usize| f.get(i).copied().unwrap_or("");
            Patch {
                group: at(0),
                name: at(1),
                regex: at(2),
                replace: at(3),
                flags: at(4),
                file: at(5),
                min: Ver::parse(at(6)),
                max: Ver::parse(at(7)),
                platform: at(8),
                arch: at(9),
            }
        })
        .collect()
}

impl Patch {
    pub fn applies(&self, groups: &[&str], ver: Ver, os: Os, arch: &str) -> bool {
        // The table's binary patches target the macOS/Linux executables; Windows logic lives in Spotify.dll.
        (os != Os::Windows || self.file != "appBinary")
            && groups.contains(&self.group)
            && self.min.is_none_or(|m| ver >= m)
            && self.max.is_none_or(|m| ver <= m)
            && (self.platform.is_empty() || self.platform.split('|').any(|p| p == os.name()))
            && (self.arch.is_empty() || self.arch.split('|').any(|a| a == arch))
    }
}

/// Applies the selected patches file by file. Returns the names that matched nothing.
pub fn apply(
    client: &Client,
    groups: &[&str],
    ver: Ver,
    js: &Path,
    modals: &Path,
) -> Result<Vec<&'static str>> {
    let mut by_file: BTreeMap<&Path, Vec<Patch>> = BTreeMap::new();
    for p in table()
        .into_iter()
        .filter(|p| p.applies(groups, ver, client.os, client.arch))
    {
        let target: &Path = match p.file {
            "xpuiJs" | "vendorXpuiJs" => js,
            "xpuiDesktopModalsJs" => modals,
            "appBinary" => &client.bin,
            other => return Err(format!("patch {} targets unknown file {other}", p.name).into()),
        };
        by_file.entry(target).or_default().push(p);
    }
    let mut missed = Vec::new();
    for (file, patches) in by_file {
        let Ok(bytes) = fs::read(file) else {
            missed.extend(patches.iter().map(|p| p.name));
            continue;
        };
        let (text, file_missed) = apply_to(translate::latin1(&bytes), &patches)?;
        missed.extend(file_missed);
        fs::write(file, translate::unlatin1(&text)?)?;
    }
    Ok(missed)
}

/// Byte-level entry point, used by the parity test against Perl.
pub fn apply_bytes(bytes: &[u8], patches: &[Patch]) -> Result<(Vec<u8>, Vec<&'static str>)> {
    let (text, missed) = apply_to(translate::latin1(bytes), patches)?;
    Ok((translate::unlatin1(&text)?, missed))
}

fn apply_to(mut text: String, patches: &[Patch]) -> Result<(String, Vec<&'static str>)> {
    let mut missed = Vec::new();
    for p in patches {
        let mut hit = false;
        for c in translate::compile(p.regex, p.replace, p.flags)
            .map_err(|e| format!("patch {}: {e}", p.name))?
        {
            if let Some(new) = c.replace(&text)? {
                text = new;
                hit = true;
            }
        }
        if !hit {
            missed.push(p.name);
        }
    }
    Ok((text, missed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parses() {
        let t = table();
        assert_eq!(t.len(), 114);
        assert!(t
            .iter()
            .all(|p| !p.name.is_empty() && !p.regex.is_empty() && !p.file.is_empty()));
    }

    #[test]
    fn gating() {
        let v = Ver([1, 3, 6, 1921]);
        let t = table();
        let count =
            |groups: &[&str], os, arch| t.iter().filter(|p| p.applies(groups, v, os, arch)).count();
        assert_eq!(count(&["updates"], Os::Mac, "arm64"), 1);
        assert_eq!(count(&["updates"], Os::Linux, "x86_64"), 0);
        assert!(t.iter().all(|p| !(p.file == "appBinary"
            && p.applies(&["ao", "na", "free"], v, Os::Windows, "x86_64"))));
    }
}
