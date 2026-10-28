//! Windows Spotify.dll regex patches from SpotX's patches.json `others.binary`.
//! SpotX decodes the DLL as Windows-1251; the patterns/replacements are ASCII, so Latin-1
//! (translate.rs) gives byte-identical output. Table: name&match&replace&fr&to&kind.

use super::super::translate::{self, latin1, unlatin1};
use crate::ver::Ver;
use crate::Result;

const TABLE: &str = include_str!("../../../assets/patches/spotx-windows-binary.txt");

struct Entry {
    name: &'static str,
    find: &'static str,
    replace: &'static str,
    fr: Option<Ver>,
    to: Option<Ver>,
    kind: &'static str,
}

fn table() -> Vec<Entry> {
    TABLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with("name&"))
        .map(|l| {
            let f: Vec<&str> = l.split('&').collect();
            Entry {
                name: f[0],
                find: f[1],
                replace: f[2],
                fr: Ver::parse(f[3]),
                to: Ver::parse(f[4]),
                kind: f.get(5).copied().unwrap_or(""),
            }
        })
        .collect()
}

/// Test-PatchVersionMatch: compare on the first three version components.
fn version_match(e: &Entry, ver: Ver) -> bool {
    let v = Ver([ver.0[0], ver.0[1], ver.0[2], 0]);
    let fr_ok = e.fr.map(|fr| fr <= v).unwrap_or(false);
    let to_ok = e.to.map(|to| to >= v).unwrap_or(true);
    fr_ok && to_ok
}

/// Applies matching patches in place; returns the names that matched nothing.
pub fn apply(bytes: &mut Vec<u8>, ver: Ver, premium: bool, block_updates: bool) -> Result<Vec<&'static str>> {
    let mut text = latin1(bytes);
    let mut missed = Vec::new();
    for e in table() {
        if e.kind == "update" && !block_updates {
            continue;
        }
        if e.kind == "slots" && premium {
            continue;
        }
        if !version_match(&e, ver) {
            continue;
        }
        // .NET `-replace` is global; reuse the translator's literal prefilter for speed.
        let mut hit = false;
        for c in translate::compile(e.find, e.replace, "g").map_err(|err| format!("binary patch {}: {err}", e.name))? {
            if let Some(new) = c.replace(&text)? {
                text = new;
                hit = true;
            }
        }
        if !hit {
            missed.push(e.name);
        }
    }
    *bytes = unlatin1(&text)?;
    Ok(missed)
}
