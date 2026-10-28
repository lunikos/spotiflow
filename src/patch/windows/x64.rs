use std::collections::HashMap;

use super::pe::*;
use super::Loc;
use crate::Result;

fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|h| u8::from_str_radix(h, 16).unwrap())
        .collect()
}
fn mask_of(len: usize, zero_spans: &[(usize, usize)]) -> Vec<u8> {
    let mut m = vec![0xFFu8; len];
    for &(start, n) in zero_spans {
        m[start..start + n].fill(0);
    }
    m
}

fn signed_byte(b: &[u8], off: i64) -> Result<i64> {
    if off < 0 || off as usize >= b.len() {
        return Err("Signed byte is outside the file".into());
    }
    let v = b[off as usize] as i64;
    Ok(if v >= 0x80 { v - 0x100 } else { v })
}

enum JKind {
    Je,
    Jne,
    Jmp,
}

fn rel_branch(b: &[u8], off: i64, kind: JKind, limit: i64) -> Result<(i64, i64)> {
    let (short, near): (u8, Option<u8>) = match kind {
        JKind::Je => (0x74, Some(0x84)),
        JKind::Jne => (0x75, Some(0x85)),
        JKind::Jmp => (0xEB, None),
    };
    let near_hit = near.is_some_and(|n| {
        off >= 0 && off + 6 <= limit && b[off as usize] == 0x0F && b[off as usize + 1] == n
    });
    let (next, disp) = if off >= 0 && off + 2 <= limit && b[off as usize] == short {
        (off + 2, signed_byte(b, off + 1)?)
    } else if near_hit {
        (off + 6, i32le(b, off as usize + 2) as i64)
    } else if matches!(kind, JKind::Jmp) && off >= 0 && off + 5 <= limit && b[off as usize] == 0xE9
    {
        (off + 5, i32le(b, off as usize + 1) as i64)
    } else {
        return Err("Expected branch".into());
    };
    Ok((next, next + disp))
}

enum Field {
    CmpRcxBl,
    MovAlRcx,
    CmpRdiBl,
}

fn field_next(b: &[u8], off: i64, kind: Field) -> Result<i64> {
    let (d8, d32): (&[u8], &[u8]) = match kind {
        Field::CmpRcxBl => (&[0x38, 0x59], &[0x38, 0x99]),
        Field::MovAlRcx => (&[0x8A, 0x41], &[0x8A, 0x81]),
        Field::CmpRdiBl => (&[0x38, 0x5F], &[0x38, 0x9F]),
    };
    if match_bytes(b, off, d8) && off + 3 <= b.len() as i64 {
        return Ok(off + 3);
    }
    if match_bytes(b, off, d32) && off + 6 <= b.len() as i64 {
        return Ok(off + 6);
    }
    Err("Unexpected block_slots field instruction".into())
}

fn dedup(mut v: Vec<i64>) -> Vec<i64> {
    v.sort_unstable();
    v.dedup();
    v
}

pub fn reset_dll_sign(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    pe.context()?;
    let (_, anchor_rva) = unique_anchor(
        bytes,
        pe,
        "Check failed: sep_pos != std::wstring::npos.",
        false,
    )?;
    let refs = dedup(find_rip_lea_refs(
        bytes,
        pe.code.rptr,
        pe.code.rsize,
        anchor_rva,
        pe,
    ));
    let patched = hex("B8 01 00 00 00 C3");
    if refs.is_empty() {
        return Err("reset_dll_sign code reference was not found".into());
    }
    let mut funcs: Vec<(i64, i64)> = Vec::new();
    for r in refs {
        let Some(rva) = pe.rva_from_off(r) else {
            continue;
        };
        if let Some(range) = func_range(bytes, pe, rva) {
            if !funcs.iter().any(|f| f.0 == range.0) {
                funcs.push(range);
            }
        }
    }
    if funcs.len() != 1 {
        return Err(format!(
            "Expected one reset_dll_sign function, found {}",
            funcs.len()
        )
        .into());
    }
    let (start, end) = funcs[0];
    let off = pe
        .off_from_rva(start)
        .ok_or("reset_dll_sign patch range is invalid")?;
    if end - start < patched.len() as i64 || off + patched.len() as i64 > bytes.len() as i64 {
        return Err("reset_dll_sign patch range is invalid".into());
    }
    let loc = if match_bytes(bytes, off, &patched) {
        Loc {
            patch_off: off,
            original: None,
            patched,
        }
    } else {
        let first = bytes[off as usize];
        if first != 0x48 && first != 0x40 && first != 0x55 && !(0x53..=0x57).contains(&first) {
            return Err("Unexpected x64 reset_dll_sign function prologue".into());
        }
        let orig = bytes[off as usize..off as usize + patched.len()].to_vec();
        Loc {
            patch_off: off,
            original: Some(orig),
            patched,
        }
    };
    Ok(vec![loc])
}

fn seq_enum(b: &[u8], seq_off: i64, branch_off: i64, match_taken: bool) -> Vec<u32> {
    let mov = hex("41 8B C8");
    if !match_bytes(b, seq_off, &mov) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for ev in 1..=0x3FFi64 {
        let mut ecx = ev;
        let mut zero = false;
        let mut has_flags = false;
        let mut cursor = seq_off + 3;
        let mut valid = true;
        let mut matched = false;
        while cursor <= branch_off {
            if match_bytes(b, cursor, &[0x83, 0xE9]) {
                ecx -= signed_byte(b, cursor + 2).unwrap_or(0);
                zero = ecx == 0;
                has_flags = true;
                cursor += 3;
                continue;
            }
            if match_bytes(b, cursor, &[0x81, 0xE9]) {
                ecx -= i32le(b, cursor as usize + 2) as i64;
                zero = ecx == 0;
                has_flags = true;
                cursor += 6;
                continue;
            }
            if match_bytes(b, cursor, &[0x83, 0xF9]) {
                zero = ecx == signed_byte(b, cursor + 2).unwrap_or(0);
                has_flags = true;
                cursor += 3;
                continue;
            }
            if match_bytes(b, cursor, &[0x81, 0xF9]) {
                zero = ecx == i32le(b, cursor as usize + 2) as i64;
                has_flags = true;
                cursor += 6;
                continue;
            }
            let (blen, on_eq);
            let c = cursor as usize;
            if cursor + 2 <= b.len() as i64 && (b[c] == 0x74 || b[c] == 0x75) {
                blen = 2;
                on_eq = b[c] == 0x74;
            } else if cursor + 6 <= b.len() as i64
                && b[c] == 0x0F
                && (b[c + 1] == 0x84 || b[c + 1] == 0x85)
            {
                blen = 6;
                on_eq = b[c + 1] == 0x84;
            } else {
                valid = false;
                break;
            }
            if !has_flags {
                valid = false;
                break;
            }
            let taken = if on_eq { zero } else { !zero };
            if cursor == branch_off {
                matched = if match_taken { taken } else { !taken };
                break;
            }
            if taken {
                valid = false;
                break;
            }
            cursor += blen;
        }
        if valid && matched {
            out.push(ev as u32);
        }
    }
    out
}

struct StringCase {
    prefix: i64,
    pattern: Vec<u8>,
    mask: Vec<u8>,
    target_mode: bool,
}

fn mapper_enum_value(b: &[u8], pe: &Pe, start_rva: i64, end_rva: i64, refs: &[i64]) -> Result<u32> {
    let mapper_off = pe
        .off_from_rva(start_rva)
        .ok_or("slot_is_disabled mapper offset was not found")?;
    let mapper_end = mapper_off + (end_rva - start_rva);
    if mapper_end <= mapper_off || mapper_end > b.len() as i64 {
        return Err("slot_is_disabled mapper range is invalid".into());
    }
    let p1 =
        hex("0F 57 C0 0F 11 02 48 89 7A 10 48 89 7A 18 41 B8 10 00 00 00 48 8D 15 00 00 00 00");
    let p2 = hex("0F 57 C0 0F 11 02 48 89 7A 10 48 89 7A 18 44 8D 41 0F 48 8D 15 00 00 00 00");
    let cases = [
        StringCase {
            prefix: 20,
            mask: mask_of(p1.len(), &[(23, 4)]),
            pattern: p1,
            target_mode: true,
        },
        StringCase {
            prefix: 18,
            mask: mask_of(p2.len(), &[(21, 4)]),
            pattern: p2,
            target_mode: false,
        },
    ];
    let cmp_r8d8 = hex("41 83 F8");
    let cmp_r8d32 = hex("41 81 F8");
    let mov = hex("41 8B C8");
    let mut enums: HashMap<i64, u32> = HashMap::new();

    for &aref in refs {
        if aref + 7 > mapper_end {
            continue;
        }
        for case in &cases {
            let case_off = aref - case.prefix;
            if case_off < mapper_off || !match_masked(b, case_off, &case.pattern, &case.mask) {
                continue;
            }
            let mut dispatches: Vec<(i64, bool, bool)> = Vec::new();
            if case.target_mode {
                let mut bo = mapper_off;
                while bo < case_off {
                    let c = bo as usize;
                    let (on_eq, target) = if (b[c] == 0x74 || b[c] == 0x75) && bo + 2 <= mapper_end
                    {
                        (b[c] == 0x74, bo + 2 + signed_byte(b, bo + 1).unwrap_or(0))
                    } else if b[c] == 0x0F
                        && bo + 6 <= mapper_end
                        && (b[c + 1] == 0x84 || b[c + 1] == 0x85)
                    {
                        (b[c + 1] == 0x84, bo + 6 + i32le(b, c + 2) as i64)
                    } else {
                        bo += 1;
                        continue;
                    };
                    if target == case_off {
                        dispatches.push((bo, on_eq, true));
                    }
                    bo += 1;
                }
            } else {
                let c = case_off;
                let (bo, on_eq, target) = if c >= mapper_off + 6
                    && b[c as usize - 6] == 0x0F
                    && (b[c as usize - 5] == 0x84 || b[c as usize - 5] == 0x85)
                {
                    (
                        c - 6,
                        b[c as usize - 5] == 0x84,
                        c + i32le(b, c as usize - 4) as i64,
                    )
                } else if c >= mapper_off + 2
                    && (b[c as usize - 2] == 0x74 || b[c as usize - 2] == 0x75)
                {
                    (
                        c - 2,
                        b[c as usize - 2] == 0x74,
                        c + signed_byte(b, c - 1).unwrap_or(0),
                    )
                } else {
                    continue;
                };
                if target < mapper_off || target >= mapper_end || target == case_off {
                    continue;
                }
                dispatches.push((bo, on_eq, false));
            }

            for (bo, on_eq, match_taken) in dispatches {
                let selects_eq = on_eq == match_taken;
                if selects_eq && bo >= mapper_off + 4 && match_bytes(b, bo - 4, &cmp_r8d8) {
                    let ev = signed_byte(b, bo - 1).unwrap_or(0);
                    if ev > 0 && ev <= 0x3FF {
                        enums.insert(ev, ev as u32);
                    }
                }
                if selects_eq && bo >= mapper_off + 7 && match_bytes(b, bo - 7, &cmp_r8d32) {
                    let ev = u32at(b, bo as usize - 4) as i64;
                    if ev > 0 && ev <= 0x3FF {
                        enums.insert(ev, ev as u32);
                    }
                }
                let seq_start = mapper_off.max(bo - 0x100);
                let mut co = seq_start;
                while co < bo {
                    if match_bytes(b, co, &mov) {
                        for e in seq_enum(b, co, bo, match_taken) {
                            enums.insert(e as i64, e);
                        }
                    }
                    co += 1;
                }
            }
        }
    }
    if enums.len() != 1 {
        return Err(format!(
            "Expected one slot_is_disabled enum value, found {}",
            enums.len()
        )
        .into());
    }
    Ok(*enums.values().next().unwrap())
}

fn block_slots_predicate(b: &[u8], pe: &Pe, target_rva: i64) -> Option<Loc> {
    (|| -> Result<Option<Loc>> {
        let Some((rb, rf)) = find_function_range(
            b,
            pe.exc_rawptr,
            pe.exc_rawsize,
            target_rva,
            pe.code.vaddr,
            pe.code.vsize.max(pe.code.rsize),
        ) else {
            return Ok(None);
        };
        if rb != target_rva {
            return Ok(None);
        }
        let fsize = rf - rb;
        if !(0x20..=0x100).contains(&fsize) {
            return Ok(None);
        }
        let Some(fo) = pe.off_from_rva(target_rva) else {
            return Ok(None);
        };
        let fend = fo + fsize;
        let tstart = pe.code.rptr;
        let tend = tstart + pe.code.rsize;
        if fo < tstart || fend > tend || fend > b.len() as i64 {
            return Ok(None);
        }
        let mut code_off = fo;
        if match_bytes(b, code_off, &hex("F3 0F 1E FA")) {
            code_off += 4;
        }
        let prologue = hex("48 89 5C 24 00 57 48 83 EC 00 32 DB");
        let pmask = mask_of(prologue.len(), &[(4, 1), (9, 1)]);
        if !match_masked(b, code_off, &prologue, &pmask) {
            return Ok(None);
        }
        let save_disp = b[code_off as usize + 4] as i64;
        let stack_frame = b[code_off as usize + 9] as i64;
        if stack_frame <= 0 {
            return Ok(None);
        }
        let patch_off = code_off + prologue.len() as i64;
        let original = hex("48 8B F9");
        let patched_state = if match_bytes(b, patch_off, &original) {
            false
        } else if patch_off + 3 <= fend
            && b[patch_off as usize] == 0xEB
            && b[patch_off as usize + 2] == 0x90
        {
            true
        } else {
            return Ok(None);
        };
        let mut cursor = patch_off + 3;
        cursor = field_next(b, cursor, Field::CmpRcxBl)?;
        let to_gate = rel_branch(b, cursor, JKind::Je, fend)?;
        cursor = to_gate.0;
        cursor = field_next(b, cursor, Field::MovAlRcx)?;
        let to_epilogue = rel_branch(b, cursor, JKind::Jmp, fend)?;
        cursor = to_epilogue.0;
        if to_gate.1 != cursor {
            return Ok(None);
        }
        cursor = field_next(b, cursor, Field::CmpRcxBl)?;
        let gate_to_false = rel_branch(b, cursor, JKind::Je, fend)?;
        cursor = gate_to_false.0;
        if cursor + 5 > fend || b[cursor as usize] != 0xE8 {
            return Ok(None);
        }
        let Some(helper_call_rva) = pe.rva_from_off(cursor) else {
            return Ok(None);
        };
        let helper_target = helper_call_rva + 5 + i32le(b, cursor as usize + 1) as i64;
        if helper_target < pe.code.vaddr || helper_target >= pe.text_rva_end() {
            return Ok(None);
        }
        cursor += 5;
        if !match_bytes(b, cursor, &hex("84 C0")) {
            return Ok(None);
        }
        cursor += 2;
        let helper_to_true = rel_branch(b, cursor, JKind::Jne, fend)?;
        cursor = helper_to_true.0;
        cursor = field_next(b, cursor, Field::CmpRdiBl)?;
        let fallback_to_false = rel_branch(b, cursor, JKind::Je, fend)?;
        cursor = fallback_to_false.0;
        let set_true = cursor;
        if helper_to_true.1 != set_true || !match_bytes(b, set_true, &hex("B3 01")) {
            return Ok(None);
        }
        cursor += 2;
        let return_false = cursor;
        if gate_to_false.1 != return_false
            || fallback_to_false.1 != return_false
            || !match_bytes(b, return_false, &hex("8A C3"))
        {
            return Ok(None);
        }
        cursor += 2;
        if to_epilogue.1 != cursor {
            return Ok(None);
        }
        let epilogue = hex("48 8B 5C 24 00 48 83 C4 00 5F C3");
        let emask = mask_of(epilogue.len(), &[(4, 1), (8, 1)]);
        if !match_masked(b, cursor, &epilogue, &emask) {
            return Ok(None);
        }
        let restore_disp = b[cursor as usize + 4] as i64;
        let restore_frame = b[cursor as usize + 8] as i64;
        if restore_frame != stack_frame || restore_disp != save_disp + stack_frame + 8 {
            return Ok(None);
        }
        cursor += epilogue.len() as i64;
        let tail = fend - cursor;
        if !(0..=16).contains(&tail) {
            return Ok(None);
        }
        for i in cursor..fend {
            if b[i as usize] != 0x90 && b[i as usize] != 0xCC {
                return Ok(None);
            }
        }
        let jump_disp = return_false - (patch_off + 2);
        if !(0..=0x7F).contains(&jump_disp) {
            return Ok(None);
        }
        let patched = vec![0xEB, jump_disp as u8, 0x90];
        if patched_state && !match_bytes(b, patch_off, &patched) {
            return Ok(None);
        }
        Ok(Some(Loc {
            patch_off,
            original: if patched_state { None } else { Some(original) },
            patched,
        }))
    })()
    .ok()
    .flatten()
}

pub fn block_slots(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    pe.context()?;
    let (_, anchor_rva) = unique_anchor(bytes, pe, "slot_is_disabled", true)?;
    let refs = dedup(find_rip_lea_refs(
        bytes,
        pe.code.rptr,
        pe.code.rsize,
        anchor_rva,
        pe,
    ));
    if refs.is_empty() {
        return Err("slot_is_disabled code reference was not found".into());
    }
    let mut funcs: Vec<(i64, i64)> = Vec::new();
    for &r in &refs {
        let rva = pe
            .rva_from_off(r)
            .ok_or("slot_is_disabled reference RVA was not found")?;
        let range =
            func_range(bytes, pe, rva).ok_or("slot_is_disabled mapper function was not found")?;
        let size = range.1 - range.0;
        if !(0x100..=0x4000).contains(&size) {
            return Err("Unexpected slot_is_disabled mapper function size".into());
        }
        if !funcs.iter().any(|f| f.0 == range.0) {
            funcs.push(range);
        }
    }
    if funcs.len() != 1 {
        return Err(format!(
            "Expected one slot_is_disabled mapper function, found {}",
            funcs.len()
        )
        .into());
    }
    let slot_enum = mapper_enum_value(bytes, pe, funcs[0].0, funcs[0].1, &refs)?;

    let patterns = [
        (
            hex("E8 00 00 00 00 84 C0 75 00 BA 00 00 00 00"),
            mask_of(14, &[(1, 4), (8, 1), (10, 4)]),
            7i64,
            10i64,
        ),
        (
            hex("E8 00 00 00 00 84 C0 0F 85 00 00 00 00 BA 00 00 00 00"),
            mask_of(18, &[(1, 4), (9, 4), (14, 4)]),
            7,
            14,
        ),
    ];
    let text_start = pe.code.rptr;
    let text_end = pe.code.rptr + pe.code.rsize;
    let mut targets: HashMap<i64, Loc> = HashMap::new();

    for (pat, mask, branch_off, enum_off) in &patterns {
        let mut search = text_start;
        while search < text_end {
            let call = find_masked(bytes, pat, mask, search, text_end - search);
            if call < 0 {
                break;
            }
            search = call + 1;
            let _ = (|| -> Result<()> {
                let ev = u32at(bytes, (call + enum_off) as usize);
                if ev != slot_enum {
                    return Ok(());
                }
                let Some(caller_rva) = pe.rva_from_off(call) else {
                    return Ok(());
                };
                let Some(call_next_rva) = pe.rva_from_off(call + 5) else {
                    return Ok(());
                };
                let Some(caller_range) = func_range(bytes, pe, caller_rva) else {
                    return Ok(());
                };
                let cb = rel_branch(bytes, call + branch_off, JKind::Jne, text_end)?;
                let Some(bt_rva) = pe.rva_from_off(cb.1) else {
                    return Ok(());
                };
                if bt_rva < caller_range.0 || bt_rva >= caller_range.1 {
                    return Ok(());
                }
                let target_rva = call_next_rva + i32le(bytes, call as usize + 1) as i64;
                if let Some(loc) = block_slots_predicate(bytes, pe, target_rva) {
                    targets.entry(target_rva).or_insert(loc);
                }
                Ok(())
            })();
        }
    }
    if targets.is_empty() {
        return Err("block_slots semantic function was not found".into());
    }
    if targets.len() != 1 {
        return Err(format!(
            "Expected one block_slots semantic function, found {}",
            targets.len()
        )
        .into());
    }
    Ok(targets.into_values().collect())
}

pub fn crossfade(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    pe.context()?;
    let (_, anchor_rva) = unique_anchor(bytes, pe, "crossfade_enabled", true)?;
    let refs = dedup(find_rip_lea_refs(
        bytes,
        pe.code.rptr,
        pe.code.rsize,
        anchor_rva,
        pe,
    ));
    if refs.is_empty() {
        return Err("No x64 code reference to crossfade_enabled was found".into());
    }
    let mut getters: Vec<(i64, i64)> = Vec::new();
    for r in refs {
        let Some(rva) = pe.rva_from_off(r) else {
            continue;
        };
        if let Some(range) = func_range(bytes, pe, rva) {
            if !getters.iter().any(|g| g.0 == range.0) {
                getters.push(range);
            }
        }
    }
    if getters.len() != 1 {
        return Err(format!(
            "Expected one x64 crossfade getter function, found {}",
            getters.len()
        )
        .into());
    }
    let getter = getters[0];

    let gate_pat = hex("48 8B 0B 00 00 00 00 00 88 45 00 48 8D 4D 00 E8");
    let gate_mask = mask_of(gate_pat.len(), &[(3, 5), (10, 1), (14, 1)]);
    let ext_pat =
        hex("48 8B 0B 00 00 00 00 00 88 45 00 48 8B 0B E8 00 00 00 00 88 45 00 48 8D 4D 00 E8");
    let ext_mask = mask_of(ext_pat.len(), &[(3, 5), (10, 1), (15, 4), (21, 1), (25, 1)]);
    let has_gate = |cand: i64| -> bool {
        let ctx_len = if match_masked(bytes, cand - 3, &gate_pat, &gate_mask) {
            gate_pat.len() as i64
        } else if match_masked(bytes, cand - 3, &ext_pat, &ext_mask) {
            ext_pat.len() as i64
        } else {
            return false;
        };
        let Some(crva) = pe.rva_from_off(cand) else {
            return false;
        };
        match func_range(bytes, pe, crva) {
            Some(range) => crva - 3 >= range.0 && crva - 3 + ctx_len <= range.1,
            None => false,
        }
    };

    let original_candidates: Vec<i64> = dedup(find_gate_calls_to_rva(
        bytes,
        pe.code.rptr,
        pe.code.rsize,
        getter.0,
        pe,
    ))
    .into_iter()
    .filter(|&c| has_gate(c))
    .collect();

    let patched = hex("B0 01 90 90 90");
    let text_end = pe.code.rptr + pe.code.rsize;
    let mut patched_candidates = Vec::new();
    let mut search = pe.code.rptr;
    loop {
        let cand = find_bytes(bytes, &patched, search);
        if cand < 0 || cand + 13 > text_end {
            break;
        }
        search = cand + 1;
        if has_gate(cand) {
            patched_candidates.push(cand);
        }
    }

    if original_candidates.len() + patched_candidates.len() != 1 {
        return Err(format!(
            "Expected one x64 crossfade gate call site, found {}",
            original_candidates.len() + patched_candidates.len()
        )
        .into());
    }

    let loc = if original_candidates.len() == 1 {
        let patch_off = original_candidates[0];
        let patch_rva = pe
            .rva_from_off(patch_off)
            .ok_or("x64 crossfade call site RVA was not found")?;
        let disp = getter.0 - (patch_rva + 5);
        if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
            return Err("x64 crossfade call target is out of range".into());
        }
        let mut original = vec![0xE8];
        original.extend_from_slice(&(disp as i32).to_le_bytes());
        if !match_bytes(bytes, patch_off, &original) {
            return Err("Unexpected x64 crossfade call bytes".into());
        }
        Loc {
            patch_off,
            original: Some(original),
            patched,
        }
    } else {
        Loc {
            patch_off: patched_candidates[0],
            original: None,
            patched,
        }
    };
    Ok(vec![loc])
}

pub fn list_player(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    pe.context()?;
    let (_, anchor_rva) = unique_anchor(bytes, pe, "enable_list_player", true)?;
    let owner_bytes = b"core-list-player\0";
    let refs = find_rip_lea_refs(bytes, pe.code.rptr, pe.code.rsize, anchor_rva, pe);
    let pattern = hex(
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 40 48 8B F9 \
         48 8D 2D 00 00 00 00 48 8B CD E8 00 00 00 00 48 8D 35 00 00 00 00 \
         48 8B D8 48 8B CE E8 00 00 00 00 45 33 C9 48 89 6C 24 20 4C 8D 44 24 20 \
         48 89 5C 24 28 48 8D 54 24 30 48 89 74 24 30 48 8B CF 48 89 44 24 38 \
         00 00 00 00 00 48 8B 5C 24 50 48 8B 6C 24 58 48 8B 74 24 60 48 83 C4 40 5F C3",
    );
    let mask = mask_of(
        pattern.len(),
        &[(0x1A, 4), (0x22, 4), (0x29, 4), (0x34, 4), (0x5C, 5)],
    );
    let patched = hex("B0 00 90 90 90");
    let optional_bool = hex("FF D7 33 FF 40 38 78 01 74 03 44 8A 30");
    let return_bool = hex("41 8A C6 48 8B 4D F8");
    let mut candidates: HashMap<i64, Loc> = HashMap::new();

    for reference in refs {
        let res: Result<()> = (|| {
            let Some(ref_rva) = pe.rva_from_off(reference) else {
                return Ok(());
            };
            let Some(range) = func_range(bytes, pe, ref_rva) else {
                return Ok(());
            };
            if ref_rva - range.0 != 0x17 || range.1 - range.0 != pattern.len() as i64 {
                return Ok(());
            }
            let Some(start) = pe.off_from_rva(range.0) else {
                return Ok(());
            };
            if !match_masked(bytes, start, &pattern, &mask) {
                return Ok(());
            }
            let owner_rva = range.0 + 0x2D + i32le(bytes, start as usize + 0x29) as i64;
            let Some(owner_off) = pe.off_from_rva(owner_rva) else {
                return Ok(());
            };
            if !match_bytes(bytes, owner_off, owner_bytes) {
                return Ok(());
            }
            let callers: Vec<i64> =
                find_gate_calls_to_rva(bytes, pe.code.rptr, pe.code.rsize, range.0, pe)
                    .into_iter()
                    .filter(|&c| {
                        let Some(crva) = pe.rva_from_off(c) else {
                            return false;
                        };
                        match func_range(bytes, pe, crva) {
                            Some(cr) => crva + 7 <= cr.1 && (bytes[c as usize + 6] & 0x38) == 0,
                            None => false,
                        }
                    })
                    .collect();
            if callers.is_empty() {
                return Ok(());
            }
            let offset = start + 0x5C;
            let (original, patched_state) = if match_bytes(bytes, offset, &patched) {
                (None, true)
            } else if bytes[offset as usize] == 0xE8 {
                let target_rva = range.0 + 0x61 + i32le(bytes, offset as usize + 1) as i64;
                let Some(tr) = func_range(bytes, pe, target_rva) else {
                    return Ok(());
                };
                if tr.0 != target_rva {
                    return Ok(());
                }
                let Some(target) = pe.off_from_rva(target_rva) else {
                    return Ok(());
                };
                let end = target + tr.1 - target_rva;
                let opt = find_bytes(bytes, &optional_bool, target);
                let ret = find_bytes(bytes, &return_bool, target);
                if opt < target
                    || opt + optional_bool.len() as i64 > end
                    || ret < opt
                    || ret + return_bool.len() as i64 > end
                {
                    return Ok(());
                }
                (
                    Some(bytes[offset as usize..offset as usize + 5].to_vec()),
                    false,
                )
            } else {
                return Ok(());
            };
            candidates.entry(offset).or_insert(Loc {
                patch_off: offset,
                original: if patched_state { None } else { original },
                patched: patched.clone(),
            });
            Ok(())
        })();
        let _ = res;
    }
    if candidates.len() != 1 {
        return Err(format!(
            "Expected one x64 ListPlayer boolean reader, found {}",
            candidates.len()
        )
        .into());
    }
    Ok(candidates.into_values().collect())
}
