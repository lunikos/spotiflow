//! ARM64 instruction decoding and the ARM64 Spotify.dll locators (SpotX run.ps1).

use super::pe::*;
use super::Loc;
use crate::Result;

pub fn read_inst(b: &[u8], off: i64) -> Result<u32> {
    if off < 0 || off as usize + 4 > b.len() || off % 4 != 0 {
        return Err("ARM64 instruction is outside the file or unaligned".into());
    }
    Ok(u32::from_le_bytes([b[off as usize], b[off as usize + 1], b[off as usize + 2], b[off as usize + 3]]))
}

fn signed_imm(value: i64, bits: u32) -> i64 {
    let sign = 1i64 << (bits - 1);
    if value & sign != 0 {
        value - (1i64 << bits)
    } else {
        value
    }
}

pub fn adr_target(inst: u32, inst_rva: i64) -> Result<i64> {
    if inst & 0x9F000000 != 0x10000000 {
        return Err("Expected ARM64 ADR instruction".into());
    }
    let imm = (((inst >> 5) & 0x7FFFF) as i64) << 2 | ((inst >> 29) & 3) as i64;
    Ok(inst_rva + signed_imm(imm, 21))
}

#[derive(Clone, Copy)]
pub enum BKind {
    B,
    Bl,
    Tbnz,
}

pub fn branch_target(inst: u32, inst_rva: i64, kind: BKind) -> Result<i64> {
    let imm = match kind {
        BKind::B => {
            if inst & 0xFC000000 != 0x14000000 {
                return Err("Expected ARM64 B instruction".into());
            }
            signed_imm((inst & 0x03FFFFFF) as i64, 26)
        }
        BKind::Bl => {
            if inst & 0xFC000000 != 0x94000000 {
                return Err("Expected ARM64 BL instruction".into());
            }
            signed_imm((inst & 0x03FFFFFF) as i64, 26)
        }
        BKind::Tbnz => {
            if inst & 0xFFF8001F != 0x37000000 {
                return Err("Expected ARM64 TBNZ w0, #0 instruction".into());
            }
            signed_imm(((inst >> 5) & 0x3FFF) as i64, 14)
        }
    };
    Ok(inst_rva + (imm << 2))
}

pub fn cbz_w8_target(inst: u32, inst_rva: i64) -> Result<i64> {
    if inst & 0xFF00001F != 0x34000008 {
        return Err("Expected ARM64 CBZ w8 instruction".into());
    }
    let imm = signed_imm(((inst >> 5) & 0x7FFFF) as i64, 19);
    Ok(inst_rva + (imm << 2))
}

pub fn branch_bytes(inst_rva: i64, target_rva: i64) -> Result<[u8; 4]> {
    let disp = target_rva - inst_rva;
    if disp % 4 != 0 || !(-0x8000000..=0x7FFFFFC).contains(&disp) {
        return Err("ARM64 branch target is out of range or unaligned".into());
    }
    let imm = ((disp / 4) & 0x03FFFFFF) as u32;
    Ok((0x14000000 | imm).to_le_bytes())
}

pub fn adrp_page_rva(ownerpage: u32, inst_rva: i64) -> i64 {
    let imm = (((ownerpage >> 5) & 0x7FFFF) as i64) << 2 | ((ownerpage >> 29) & 3) as i64;
    (inst_rva & -4096i64) + (signed_imm(imm, 21) << 12)
}

// --- reset_dll_sign (ARM64 half) ---
pub fn reset_dll_sign(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    pe.context()?;
    let (_, anchor_rva) = unique_anchor(bytes, pe, "Check failed: sep_pos != std::wstring::npos.", false)?;
    let refs: Vec<i64> =
        dedup(find_xref_arm64(bytes, anchor_rva as u64, pe.code.vaddr, pe.code.rptr as u32, pe.code.rsize as u32));
    let patched = vec![0x20, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6];
    if refs.is_empty() {
        return Err("reset_dll_sign code reference was not found".into());
    }
    let mut funcs: Vec<(i64, i64)> = Vec::new();
    for r in refs {
        let Some(rva) = pe.rva_from_off(r) else { continue };
        if let Some(range) = func_range(bytes, pe, rva) {
            if !funcs.iter().any(|f| f.0 == range.0) {
                funcs.push(range);
            }
        }
    }
    if funcs.len() != 1 {
        return Err(format!("Expected one reset_dll_sign function, found {}", funcs.len()).into());
    }
    let start = funcs[0].0;
    let end = funcs[0].1;
    let off = pe.off_from_rva(start).ok_or("reset_dll_sign patch range is invalid")?;
    if end - start < patched.len() as i64 || off + patched.len() as i64 > bytes.len() as i64 {
        return Err("reset_dll_sign patch range is invalid".into());
    }
    let loc = if match_bytes(bytes, off, &patched) {
        Loc { patch_off: off, original: None, patched }
    } else {
        let prologue = read_inst(bytes, off)?;
        if prologue & 0xFF00FFFF != 0xA9007BFD {
            return Err("Unexpected ARM64 reset_dll_sign function prologue".into());
        }
        let orig = bytes[off as usize..off as usize + patched.len()].to_vec();
        Loc { patch_off: off, original: Some(orig), patched }
    };
    Ok(vec![loc])
}

fn dedup(mut v: Vec<i64>) -> Vec<i64> {
    v.sort_unstable();
    v.dedup();
    v
}

// --- block_slots (ARM64) ---
fn mapper_enum_value(bytes: &[u8], pe: &Pe, start_rva: i64, end_rva: i64, refs: &[i64]) -> Result<u32> {
    let mapper_off = pe.off_from_rva(start_rva).ok_or("slot_is_disabled mapper offset was not found")?;
    let mapper_end = mapper_off + (end_rva - start_rva);
    if mapper_end <= mapper_off || mapper_end > bytes.len() as i64 {
        return Err("slot_is_disabled mapper range is invalid".into());
    }
    let search_end = (mapper_off + 0x80).min(mapper_end - 28);
    let mut dispatchers: Vec<(i64, i64, i64)> = Vec::new(); // (enum_limit, table_rva, base_rva)
    let mut off = mapper_off;
    while off <= search_end {
        let cmp = read_inst(bytes, off)?;
        let branch = read_inst(bytes, off + 4)?;
        let table_adr = read_inst(bytes, off + 8)?;
        let load = read_inst(bytes, off + 12)?;
        let base_adr = read_inst(bytes, off + 16)?;
        let add = read_inst(bytes, off + 20)?;
        let dispatch = read_inst(bytes, off + 24)?;
        if cmp & 0xFFC003FF == 0x7100005F
            && branch & 0xFF00001F == 0x54000008
            && table_adr & 0x9F00001F == 0x10000009
            && load == 0xB8A25928
            && base_adr & 0x9F00001F == 0x10000009
            && add == 0x8B080928
            && dispatch == 0xD61F0100
        {
            let drva = pe.rva_from_off(off).ok_or("dispatcher rva")?;
            dispatchers.push((
                ((cmp >> 10) & 0xFFF) as i64,
                adr_target(table_adr, drva + 8)?,
                adr_target(base_adr, drva + 16)?,
            ));
        }
        off += 4;
    }
    if dispatchers.len() != 1 {
        return Err(format!("Expected one ARM64 slot mapper dispatcher, found {}", dispatchers.len()).into());
    }
    let (enum_limit, table_rva, base_rva) = dispatchers[0];
    let mut anchor_rvas = std::collections::HashSet::new();
    for &r in refs {
        if let Some(rva) = pe.rva_from_off(r) {
            anchor_rvas.insert(rva);
        }
    }
    let table_off = pe.off_from_rva(table_rva).ok_or("slot_is_disabled jump table offset was not found")?;
    let table_section = pe.sections.iter().find(|s| table_rva >= s.vaddr && table_rva < s.vaddr + s.rsize);
    let table_end = table_off + (enum_limit + 1) * 4;
    let ok = table_section.map(|s| table_end <= s.rptr + s.rsize).unwrap_or(false) && table_end <= bytes.len() as i64;
    if !ok {
        return Err("slot_is_disabled jump table range is invalid".into());
    }
    let mut found = Vec::new();
    for ev in 0..=enum_limit {
        let entry = table_off + ev * 4;
        let rel = i32le(bytes, entry as usize) as i64;
        let target = base_rva + rel * 4;
        if anchor_rvas.contains(&target) {
            found.push(ev as u32);
        }
    }
    if found.len() != 1 {
        return Err(format!("Expected one ARM64 slot_is_disabled enum value, found {}", found.len()).into());
    }
    Ok(found[0])
}

fn block_slots_predicate(bytes: &[u8], pe: &Pe, target_rva: i64) -> Option<Loc> {
    (|| -> Result<Option<Loc>> {
        let range = func_range(bytes, pe, target_rva);
        let Some((b, f)) = range else { return Ok(None) };
        if b != target_rva || f - b != 0x54 {
            return Ok(None);
        }
        let fo = match pe.off_from_rva(target_rva) {
            Some(o) if o + 0x54 <= bytes.len() as i64 => o,
            _ => return Ok(None),
        };
        let mut inst = [0u32; 21];
        for (k, slot) in inst.iter_mut().enumerate() {
            *slot = read_inst(bytes, fo + (k as i64) * 4)?;
        }
        if inst[0] != 0xF81F0FF3 || inst[1] != 0xA9BF7BFD || inst[2] != 0x910003FD {
            return Ok(None);
        }
        let return_false_rva = target_rva + 0x44;
        let return_false_off = fo + 0x44;
        let patch_off = fo + 0x0C;
        let original = vec![0xF3, 0x03, 0x00, 0xAA];
        let patched = branch_bytes(target_rva + 0x0C, return_false_rva)?.to_vec();
        let patched_state = if match_bytes(bytes, patch_off, &original) {
            false
        } else if match_bytes(bytes, patch_off, &patched) {
            true
        } else {
            return Ok(None);
        };
        if inst[4] & 0xFFC003FF != 0x39400268
            || inst[6] & 0xFFC003FF != 0x39400260
            || inst[8] & 0xFFC003FF != 0x39400268
            || inst[13] & 0xFFC003FF != 0x39400268
        {
            return Ok(None);
        }
        if cbz_w8_target(inst[5], target_rva + 0x14)? != target_rva + 0x20
            || branch_target(inst[7], target_rva + 0x1C, BKind::B)? != target_rva + 0x48
            || cbz_w8_target(inst[9], target_rva + 0x24)? != return_false_rva
            || branch_target(inst[12], target_rva + 0x30, BKind::Tbnz)? != target_rva + 0x3C
            || cbz_w8_target(inst[14], target_rva + 0x38)? != return_false_rva
            || branch_target(inst[16], target_rva + 0x40, BKind::B)? != target_rva + 0x48
        {
            return Ok(None);
        }
        if inst[10] != 0xAA1303E0
            || inst[11] & 0xFC000000 != 0x94000000
            || inst[15] != 0x52800020
            || inst[17] != 0x52800000
            || inst[18] != 0xA8C17BFD
            || inst[19] != 0xF84107F3
            || inst[20] != 0xD65F03C0
        {
            return Ok(None);
        }
        let helper = branch_target(inst[11], target_rva + 0x2C, BKind::Bl)?;
        if helper < pe.code.vaddr || helper >= pe.text_rva_end() {
            return Ok(None);
        }
        let _ = return_false_off;
        Ok(Some(Loc { patch_off, original: if patched_state { None } else { Some(original) }, patched }))
    })()
    .ok()
    .flatten()
}

pub fn block_slots(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    pe.context()?;
    let (_, anchor_rva) = unique_anchor(bytes, pe, "slot_is_disabled", true)?;
    let refs =
        dedup(find_xref_arm64(bytes, anchor_rva as u64, pe.code.vaddr, pe.code.rptr as u32, pe.code.rsize as u32));
    if refs.is_empty() {
        return Err("slot_is_disabled ARM64 code reference was not found".into());
    }
    let mut mappers: Vec<(i64, i64)> = Vec::new();
    for &r in &refs {
        let Some(rva) = pe.rva_from_off(r) else { continue };
        let Some(range) = func_range(bytes, pe, rva) else { continue };
        let size = range.1 - range.0;
        if !(0x100..=0x4000).contains(&size) {
            continue;
        }
        if !mappers.iter().any(|m| m.0 == range.0) {
            mappers.push(range);
        }
    }
    if mappers.len() != 1 {
        return Err(format!("Expected one ARM64 slot mapper function, found {}", mappers.len()).into());
    }
    let enum_value = mapper_enum_value(bytes, pe, mappers[0].0, mappers[0].1, &refs)?;
    let callers = find_arm64_block_slots_callers(bytes, pe.code.rptr, pe.code.rsize, enum_value);
    let mut targets: std::collections::HashMap<i64, Loc> = std::collections::HashMap::new();
    for co in callers {
        let res: Result<()> = (|| {
            let Some(crva) = pe.rva_from_off(co) else { return Ok(()) };
            let Some(crange) = func_range(bytes, pe, crva) else { return Ok(()) };
            if crva + 12 > crange.1 {
                return Ok(());
            }
            let branch = read_inst(bytes, co + 4)?;
            let bt = branch_target(branch, crva + 4, BKind::Tbnz)?;
            if bt < crange.0 || bt >= crange.1 {
                return Ok(());
            }
            let call = read_inst(bytes, co)?;
            let trva = branch_target(call, crva, BKind::Bl)?;
            if let Some(loc) = block_slots_predicate(bytes, pe, trva) {
                targets.entry(trva).or_insert(loc);
            }
            Ok(())
        })();
        let _ = res;
    }
    if targets.len() != 1 {
        return Err(format!("Expected one ARM64 block_slots semantic function, found {}", targets.len()).into());
    }
    Ok(targets.into_values().collect())
}

// --- crossfade_enabled (ARM64) ---
pub fn crossfade(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    pe.context()?;
    let (_, anchor_rva) = unique_anchor(bytes, pe, "crossfade_enabled", true)?;
    let refs =
        dedup(find_xref_arm64(bytes, anchor_rva as u64, pe.code.vaddr, pe.code.rptr as u32, pe.code.rsize as u32));
    if refs.is_empty() {
        return Err("No ARM64 code reference to crossfade_enabled was found".into());
    }
    let mut ref_funcs: std::collections::HashMap<i64, (i64, i64)> = std::collections::HashMap::new();
    for &r in &refs {
        let Some(rva) = pe.rva_from_off(r) else { continue };
        if let Some(range) = func_range(bytes, pe, rva) {
            ref_funcs.insert(r, range);
        }
    }
    let patched = vec![0x20, 0x00, 0x80, 0x52];
    let mut candidates: std::collections::HashMap<i64, Loc> = std::collections::HashMap::new();
    for &r in &refs {
        let res: Result<()> = (|| {
            let Some(&function) = ref_funcs.get(&r) else { return Ok(()) };
            let add_off = r + 4;
            let add_anchor = read_inst(bytes, add_off)?;
            if add_anchor & 0xFFC0001F != 0x91000003 {
                return Ok(());
            }
            let mut mov_key = read_inst(bytes, add_off + 4)?;
            let mov_owner;
            let mov_default;
            let mov_resolver;
            let add_other_key;
            let patch_off;
            let result_off;
            if mov_key & 0x9F000000 == 0x90000000 {
                let owner_page = mov_key;
                add_other_key = read_inst(bytes, add_off + 8)?;
                mov_key = read_inst(bytes, add_off + 12)?;
                mov_default = read_inst(bytes, add_off + 16)?;
                mov_resolver = read_inst(bytes, add_off + 20)?;
                mov_owner = read_inst(bytes, add_off + 24)?;
                patch_off = add_off + 28;
                let owner_reload = read_inst(bytes, add_off + 32)?;
                result_off = add_off + 36;
                if ((add_other_key >> 5) & 31) != (owner_page & 31)
                    || owner_reload & 0xFFC0001F != (0xF9400000 | ((mov_owner >> 16) & 31))
                {
                    return Ok(());
                }
            } else {
                mov_owner = read_inst(bytes, add_off + 8)?;
                mov_default = read_inst(bytes, add_off + 12)?;
                mov_resolver = read_inst(bytes, add_off + 16)?;
                add_other_key = read_inst(bytes, add_off + 20)?;
                patch_off = add_off + 24;
                result_off = add_off + 28;
            }
            let patch_inst = read_inst(bytes, patch_off)?;
            let store_result = read_inst(bytes, result_off)?;
            if mov_key != 0xAA0003E2
                || mov_owner & 0xFFE0FFFF != 0xAA0003E0
                || mov_default != 0x52800025
                || mov_resolver & 0xFFE0FFFF != 0xAA0003E4
                || add_other_key & 0xFFC0001F != 0x91000001
                || store_result & 0xFFFFFFE0 != 0x2A0003E0
            {
                return Ok(());
            }
            let Some(patch_rva) = pe.rva_from_off(patch_off) else { return Ok(()) };
            if patch_rva < function.0 || patch_rva + result_off - patch_off + 4 > function.1 {
                return Ok(());
            }
            let (original, patched_state) = if patch_inst & 0xFC000000 == 0x94000000 {
                let target = branch_target(patch_inst, patch_rva, BKind::Bl)?;
                if target < pe.code.vaddr || target >= pe.text_rva_end() {
                    return Ok(());
                }
                (Some(patch_inst.to_le_bytes().to_vec()), false)
            } else if patch_inst == 0x52800020 {
                (None, true)
            } else {
                return Ok(());
            };
            candidates.entry(patch_off).or_insert(Loc {
                patch_off,
                original: if patched_state { None } else { original },
                patched: patched.clone(),
            });
            Ok(())
        })();
        let _ = res;
    }
    if candidates.len() != 1 {
        return Err(format!("Expected one ARM64 crossfade boolean call site, found {}", candidates.len()).into());
    }
    Ok(candidates.into_values().collect())
}

// --- ListPlayer (ARM64), only >= 1.2.98 ---
pub fn list_player(bytes: &[u8], pe: &Pe) -> Result<Vec<Loc>> {
    pe.context()?;
    let (_, anchor_rva) = unique_anchor(bytes, pe, "enable_list_player", true)?;
    let refs = find_xref_arm64(bytes, anchor_rva as u64, pe.code.vaddr, pe.code.rptr as u32, pe.code.rsize as u32);
    let owner_bytes = b"core-list-player\0";
    let patched = vec![0x00, 0x00, 0x80, 0x52];
    let optional_bool = hex("80 02 3F D6 08 04 40 39 48 00 00 34 13 00 40 39");
    let return_bool = hex("E0 03 13 2A FF 43 01 91");
    let registration = hex("13 41 40 F9 F4 03 05 2A");
    let mut covered = std::collections::HashSet::new();
    let mut candidates: std::collections::HashMap<i64, Loc> = std::collections::HashMap::new();

    for &reference in &refs {
        if covered.contains(&reference) {
            continue;
        }
        let res: Result<()> = (|| {
            let Some(refrva) = pe.rva_from_off(reference) else { return Ok(()) };
            let Some(range) = func_range(bytes, pe, refrva) else { return Ok(()) };
            if refrva + 0x44 > range.1 {
                return Ok(());
            }
            let key_add = read_inst(bytes, reference + 4)?;
            if key_add & 31 != 0 {
                return Ok(());
            }
            for distance in [0x10i64, 0x14, 0x18] {
                let owner_ref = reference + distance;
                let owner_page = read_inst(bytes, owner_ref)?;
                let owner_add = read_inst(bytes, owner_ref + 4)?;
                if owner_page & 0x9F000000 != 0x90000000
                    || owner_add & 0xFFC0001F != 0x91000000
                    || ((owner_add >> 5) & 31) != (owner_page & 31)
                {
                    continue;
                }
                let owner_rva = adrp_page_rva(owner_page, refrva + distance) + ((owner_add >> 10) & 0xFFF) as i64;
                let Some(owner_off) = pe.off_from_rva(owner_rva) else { continue };
                if !match_bytes(bytes, owner_off, owner_bytes) {
                    continue;
                }
                let save_length = read_inst(bytes, owner_ref - 4)?;
                let length_reg = save_length & 31;
                if save_length & 0xFFFFFFE0 != 0xAA0003E0 || !(19..=28).contains(&length_reg) {
                    continue;
                }
                let key_len_call = read_inst(bytes, owner_ref - 8)?;
                let owner_len_call = read_inst(bytes, owner_ref + 8)?;
                if key_len_call & 0xFC000000 != 0x94000000 || owner_len_call & 0xFC000000 != 0x94000000 {
                    continue;
                }
                let key_len_target = branch_target(key_len_call, refrva + distance - 8, BKind::Bl)?;
                let owner_len_target = branch_target(owner_len_call, refrva + distance + 8, BKind::Bl)?;
                if key_len_target != owner_len_target {
                    continue;
                }
                let mut setup = [0u32; 8];
                for (k, slot) in setup.iter_mut().enumerate() {
                    *slot = read_inst(bytes, owner_ref + 12 + 4 * k as i64)?;
                }
                let mut reload_ref: Option<i64> = None;
                let (resolver, patch_off);
                if setup[0] == 0xAA0003E2 {
                    let key_reg = (key_add >> 5) & 31;
                    let owner_reg = owner_page & 31;
                    if !(19..=28).contains(&key_reg)
                        || !(19..=28).contains(&owner_reg)
                        || setup[2] != 0x52800005
                        || setup[3] != (0xAA0003E4 | (length_reg << 16))
                        || setup[4] != (key_add | 3)
                        || setup[5] != (owner_add | 1)
                    {
                        continue;
                    }
                    resolver = setup[1];
                    patch_off = owner_ref + 36;
                } else {
                    reload_ref = Some(owner_ref + 12);
                    if !refs.contains(&(owner_ref + 12))
                        || setup[1] != (key_add | 3)
                        || setup[2] != owner_page
                        || setup[3] != (owner_add | 1)
                        || setup[4] != 0xAA0003E2
                        || setup[5] != 0x52800005
                        || setup[6] != (0xAA0003E4 | (length_reg << 16))
                    {
                        continue;
                    }
                    resolver = setup[7];
                    patch_off = owner_ref + 44;
                }
                let resolver_move = resolver & 0xFFE0FFFF == 0xAA0003E0;
                let resolver_load = resolver & 0xFFFFFC1F == 0xF9400000;
                if !resolver_move && !resolver_load {
                    continue;
                }
                let patch_rva = refrva + patch_off - reference;
                if patch_rva + 12 > range.1 {
                    continue;
                }
                let instruction = read_inst(bytes, patch_off)?;
                let next = read_inst(bytes, patch_off + 4)?;
                let after_next = read_inst(bytes, patch_off + 8)?;
                let uses_bool = next & 0xFFFFFFE0 == 0x2A0003E0
                    || next & 0xFFFFFFE0 == 0x53001C00
                    || next & 0xFFF8001F == 0x36000000
                    || (next & 0xFFC0001F == 0xF9400008 && after_next & 0xFFFFFFE0 == 0x53001C00);
                let mut original: Option<Vec<u8>> = None;
                let patched_state;
                if instruction == 0x52800000 && uses_bool {
                    patched_state = true;
                } else if instruction & 0xFC000000 == 0x94000000 {
                    let target = branch_target(instruction, patch_rva, BKind::Bl)?;
                    let Some(trange) = func_range(bytes, pe, target) else { continue };
                    if trange.0 != target {
                        continue;
                    }
                    let Some(tgt) = pe.off_from_rva(target) else { continue };
                    let end = tgt + trange.1 - target;
                    let opt = find_bytes(bytes, &optional_bool, tgt);
                    let ret = find_bytes(bytes, &return_bool, tgt);
                    if uses_bool
                        && opt >= tgt
                        && opt + optional_bool.len() as i64 <= end
                        && ret > opt
                        && ret + return_bool.len() as i64 <= end
                    {
                        patched_state = false;
                        original = Some(instruction.to_le_bytes().to_vec());
                    } else {
                        let reg = find_bytes(bytes, &registration, tgt);
                        if !uses_bool && reg >= tgt && reg + registration.len() as i64 <= end {
                            covered.insert(reference);
                        }
                        continue;
                    }
                } else {
                    continue;
                }
                covered.insert(reference);
                if let Some(rr) = reload_ref {
                    covered.insert(rr);
                }
                candidates.entry(patch_off).or_insert(Loc {
                    patch_off,
                    original: if patched_state { None } else { original },
                    patched: patched.clone(),
                });
            }
            Ok(())
        })();
        let _ = res;
    }
    if candidates.is_empty() || covered.len() != refs.len() {
        return Err(format!(
            "Unrecognized ARM64 ListPlayer readers: {}/{} references resolved",
            covered.len(),
            refs.len()
        )
        .into());
    }
    Ok(candidates.into_values().collect())
}

fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace().map(|h| u8::from_str_radix(h, 16).unwrap()).collect()
}
