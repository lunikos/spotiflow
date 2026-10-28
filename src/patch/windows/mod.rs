mod arm64;
mod pe;
mod regex;
mod x64;

use std::fs;
use std::path::Path;

use crate::ver::Ver;
use crate::Result;
use pe::{Arch, Pe};

pub struct Loc {
    pub patch_off: i64,
    pub original: Option<Vec<u8>>,
    pub patched: Vec<u8>,
}

const DLL_MIN: Ver = Ver([1, 2, 70, 253]);
const BLOCK_SLOTS_MIN: Ver = Ver([1, 2, 94, 0]);
const CROSSFADE_MIN: Ver = Ver([1, 2, 89, 0]);
const LIST_PLAYER_MIN: Ver = Ver([1, 2, 98, 0]);

fn apply_locs(bytes: &mut [u8], name: &str, mut locs: Vec<Loc>) -> Result<bool> {
    if locs.is_empty() {
        return Err(format!("No {name} patch locations were found").into());
    }
    locs.sort_by_key(|l| l.patch_off);
    let mut prev_end = -1i64;
    for l in &locs {
        if l.patched.is_empty()
            || l.patch_off < 0
            || l.patch_off + l.patched.len() as i64 > bytes.len() as i64
        {
            return Err(format!("{name} patch range is invalid").into());
        }
        match &l.original {
            None => {
                if !pe::match_bytes(bytes, l.patch_off, &l.patched) {
                    return Err(format!("Unexpected {name} patched state").into());
                }
            }
            Some(orig) => {
                if orig.len() != l.patched.len() || !pe::match_bytes(bytes, l.patch_off, orig) {
                    return Err(format!("Unexpected {name} patch state").into());
                }
            }
        }
        if l.patch_off < prev_end {
            return Err(format!("{name} patch locations overlap").into());
        }
        prev_end = l.patch_off + l.patched.len() as i64;
    }
    if locs.iter().all(|l| l.original.is_none()) {
        return Ok(false);
    }
    for l in &locs {
        let o = l.patch_off as usize;
        bytes[o..o + l.patched.len()].copy_from_slice(&l.patched);
    }
    Ok(true)
}

fn reset_dll_sign(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    match pe.arch {
        Arch::X64 => x64::reset_dll_sign(bytes, pe),
        Arch::Arm64 => arm64::reset_dll_sign(bytes, pe),
    }
}

fn block_slots(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    match pe.arch {
        Arch::X64 => x64::block_slots(bytes, pe),
        Arch::Arm64 => arm64::block_slots(bytes, pe),
    }
}

fn crossfade(bytes: &[u8], pe: &Pe, ver: Ver) -> Result<Vec<Loc>> {
    let mut locs = match pe.arch {
        Arch::X64 => x64::crossfade(bytes, pe)?,
        Arch::Arm64 => arm64::crossfade(bytes, pe)?,
    };
    if ver >= LIST_PLAYER_MIN {
        let lp = match pe.arch {
            Arch::X64 => x64::list_player(bytes, pe)?,
            Arch::Arm64 => arm64::list_player(bytes, pe)?,
        };
        locs.extend(lp);
    }
    Ok(locs)
}

fn run_pe(
    bytes: &mut [u8],
    name: &'static str,
    locate: impl Fn(&[u8], &Pe) -> Result<Vec<Loc>>,
    missed: &mut Vec<&'static str>,
) {
    let out = Pe::parse(bytes)
        .and_then(|pe| locate(bytes, &pe))
        .and_then(|locs| apply_locs(bytes, name, locs));
    if out.is_err() {
        missed.push(name);
    }
}

pub fn patch_windows(
    root: &Path,
    ver: Ver,
    premium: bool,
    block_updates: bool,
) -> Result<Vec<&'static str>> {
    if ver < DLL_MIN {
        return Err(format!("Windows binary patches need Spotify >= {DLL_MIN}").into());
    }
    let dll = root.join("Spotify.dll");
    let mut bytes = fs::read(&dll)?;

    let pe = Pe::parse(&bytes)?;
    let locs = reset_dll_sign(&bytes, &pe)?;
    apply_locs(&mut bytes, "reset_dll_sign", locs)?;

    pe::remove_sign(&mut bytes)?;
    for name in ["Spotify.exe", "chrome_elf.dll"] {
        let path = root.join(name);
        let mut b = fs::read(&path)?;
        pe::remove_sign(&mut b)?;
        fs::write(&path, &b)?;
    }

    let mut missed = regex::apply(&mut bytes, ver, premium, block_updates)?;

    if !premium && ver >= BLOCK_SLOTS_MIN {
        run_pe(&mut bytes, "block_slots", block_slots, &mut missed);
    }
    if !premium && ver >= CROSSFADE_MIN {
        run_pe(
            &mut bytes,
            "crossfade_enabled",
            |b, p| crossfade(b, p, ver),
            &mut missed,
        );
    }

    fs::write(&dll, &bytes)?;
    Ok(missed)
}

#[cfg(test)]
pub use pe::remove_sign;
