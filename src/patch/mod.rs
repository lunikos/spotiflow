mod translate;
pub mod windows;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::platform::{Client, Os};
use crate::ver::Ver;
use crate::Result;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Ads,
    NoAds,
    Free,
    Experiments,
    PremiumExperiments,
    Podcasts,
    Updates,
    NoVideos,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Target {
    Js,
    Modals,
    Binary,
}

pub struct Patch {
    pub group: Group,
    pub name: &'static str,
    pub regex: &'static str,
    pub replace: &'static str,
    pub flags: &'static str,
    pub target: Target,
    min: Option<Ver>,
    max: Option<Ver>,
    platform: &'static str,
    arch: &'static str,
}

const OWN: &str = include_str!("../../assets/patches/spotiflow.txt");
const ABOUT: &str = concat!("$1\"", include_str!("../../assets/about.html"), "\"");

pub fn table() -> Result<Vec<Patch>> {
    let mut patches = parse(crate::upstream::data().patches)?;
    patches.extend(parse(OWN)?);
    for p in patches.iter_mut().filter(|p| p.name == "aboutSpotX") {
        p.replace = ABOUT;
    }
    Ok(patches)
}

pub fn validate(table: &'static str) -> Result<usize> {
    let patches = parse(table)?;
    for p in &patches {
        translate::compile(p.regex, p.replace, p.flags)
            .map_err(|e| format!("patch {}: {e}", p.name))?;
    }
    Ok(patches.len())
}

fn parse(table: &'static str) -> Result<Vec<Patch>> {
    table
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let f: Vec<&'static str> = l.split('&').collect();
            let at = |i: usize| f.get(i).copied().unwrap_or("");
            let group = match at(0) {
                "ao" => Group::Ads,
                "na" => Group::NoAds,
                "free" => Group::Free,
                "exp" => Group::Experiments,
                "premium" => Group::PremiumExperiments,
                "pod" => Group::Podcasts,
                "updates" => Group::Updates,
                "novideo" => Group::NoVideos,
                g => return Err(format!("patch {}: unknown group {g}", at(1)).into()),
            };
            let target = match at(5) {
                "xpuiJs" | "vendorXpuiJs" => Target::Js,
                "xpuiDesktopModalsJs" => Target::Modals,
                "appBinary" => Target::Binary,
                t => return Err(format!("patch {}: unknown file {t}", at(1)).into()),
            };
            Ok(Patch {
                group,
                name: at(1),
                regex: at(2),
                replace: at(3),
                flags: at(4),
                target,
                min: Ver::parse(at(6)),
                max: Ver::parse(at(7)),
                platform: at(8),
                arch: at(9),
            })
        })
        .collect()
}

impl Patch {
    pub fn applies(&self, groups: &[Group], ver: Ver, os: Os, arch: &str) -> bool {
        // Windows binaries are handled by patch::windows; these records target macOS and Linux.
        (os != Os::Windows || self.target != Target::Binary)
            && groups.contains(&self.group)
            && self.min.is_none_or(|m| ver >= m)
            && self.max.is_none_or(|m| ver <= m)
            && (self.platform.is_empty() || self.platform.split('|').any(|p| p == os.name()))
            && (self.arch.is_empty() || self.arch.split('|').any(|a| a == arch))
    }
}

pub fn apply(
    client: &Client,
    groups: &[Group],
    ver: Ver,
    js: &Path,
    modals: &Path,
) -> Result<Vec<&'static str>> {
    let mut by_file: BTreeMap<&Path, Vec<Patch>> = BTreeMap::new();
    for p in table()?
        .into_iter()
        .filter(|p| p.applies(groups, ver, client.os, client.arch))
    {
        let target: &Path = match p.target {
            Target::Js => js,
            Target::Modals => modals,
            Target::Binary => &client.bin,
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
        let t = table().unwrap();
        assert_eq!(t.len(), 115);
        assert!(t.iter().all(|p| !p.name.is_empty() && !p.regex.is_empty()));
    }

    #[test]
    fn gating() {
        let v = Ver([1, 3, 6, 1921]);
        let t = table().unwrap();
        let count = |groups: &[Group], os, arch| {
            t.iter().filter(|p| p.applies(groups, v, os, arch)).count()
        };
        assert_eq!(count(&[Group::Updates], Os::Mac, "arm64"), 1);
        assert_eq!(count(&[Group::Updates], Os::Linux, "x86_64"), 0);
        let all = [Group::Ads, Group::NoAds, Group::Free];
        assert!(t
            .iter()
            .all(|p| !(p.target == Target::Binary && p.applies(&all, v, Os::Windows, "x86_64"))));
    }
}
