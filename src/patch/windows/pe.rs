use crate::Result;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Arch {
    X64,
    Arm64,
}

#[derive(Clone)]
pub struct Section {
    pub vsize: i64,
    pub vaddr: i64,
    pub rsize: i64,
    pub rptr: i64,
}

pub struct Pe {
    pub arch: Arch,
    pub opt_off: usize,
    pub data_dir_off: usize,
    pub sections: Vec<Section>,
    pub code: Section,
    pub exc_rawptr: i64,
    pub exc_rawsize: i64,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
pub fn i32le(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
pub fn u32at(b: &[u8], o: usize) -> u32 {
    u32le(b, o)
}

impl Pe {
    pub fn parse(bytes: &[u8]) -> Result<Pe> {
        let pe = u32le(bytes, 0x3c) as usize;
        if bytes.get(pe) != Some(&0x50) || bytes.get(pe + 1) != Some(&0x45) {
            return Err("Invalid PE file".into());
        }
        let file_header = pe + 4;
        let opt_off = file_header + 20;
        let machine = u16le(bytes, file_header);
        let (arch, data_dir_off) = match machine {
            0x8664 => (Arch::X64, 112usize),
            0xAA64 => (Arch::Arm64, 112usize),
            _ => return Err(format!("unsupported PE machine type {machine:#x}").into()),
        };
        match u16le(bytes, opt_off) {
            0x20b | 0x10b => {}
            _ => return Err("Unsupported optional header format".into()),
        }
        let num_sections = u16le(bytes, file_header + 2) as usize;
        let opt_size = u16le(bytes, file_header + 16) as usize;
        let section_table = opt_off + opt_size;

        let dir = opt_off + data_dir_off;
        let (exc_rva, exc_size) = if data_dir_off + 32 <= opt_size {
            (u32le(bytes, dir + 24) as i64, u32le(bytes, dir + 28) as i64)
        } else {
            (0, 0)
        };

        let mut sections = Vec::with_capacity(num_sections);
        let mut code: Option<Section> = None;
        for i in 0..num_sections {
            let so = section_table + i * 40;
            let s = Section {
                vsize: u32le(bytes, so + 8) as i64,
                vaddr: u32le(bytes, so + 12) as i64,
                rsize: u32le(bytes, so + 16) as i64,
                rptr: u32le(bytes, so + 20) as i64,
            };
            if (u32le(bytes, so + 36) & 0x20) != 0 && code.is_none() {
                code = Some(s.clone());
            }
            sections.push(s);
        }
        let code = code.ok_or("PE code section not found")?;

        let mut exc_rawptr = 0i64;
        let mut exc_rawsize = 0i64;
        for s in &sections {
            let span = s.vsize.max(s.rsize);
            if exc_rva < s.vaddr || exc_rva >= s.vaddr + span {
                continue;
            }
            let rel = exc_rva - s.vaddr;
            if rel < 0 || rel + exc_size > s.rsize || s.rptr + rel + exc_size > bytes.len() as i64 {
                break;
            }
            exc_rawptr = s.rptr + rel;
            exc_rawsize = exc_size;
            break;
        }

        Ok(Pe {
            arch,
            opt_off,
            data_dir_off,
            sections,
            code,
            exc_rawptr,
            exc_rawsize,
        })
    }

    pub fn rva_from_off(&self, off: i64) -> Option<i64> {
        for s in &self.sections {
            if off >= s.rptr && off < s.rptr + s.rsize {
                return Some(s.vaddr + (off - s.rptr));
            }
        }
        None
    }

    pub fn off_from_rva(&self, rva: i64) -> Option<i64> {
        for s in &self.sections {
            let span = s.vsize.max(s.rsize);
            if rva < s.vaddr || rva >= s.vaddr + span {
                continue;
            }
            let rel = rva - s.vaddr;
            if rel >= s.rsize {
                return None;
            }
            return Some(s.rptr + rel);
        }
        None
    }

    pub fn text_rva_end(&self) -> i64 {
        self.code.vaddr + self.code.vsize.max(self.code.rsize)
    }

    pub fn context(&self) -> Result<()> {
        let unit = if self.arch == Arch::Arm64 { 8 } else { 12 };
        if self.exc_rawsize < unit || self.exc_rawsize % unit != 0 {
            return Err("PE exception data size is invalid".into());
        }
        Ok(())
    }
}

pub fn remove_sign(bytes: &mut [u8]) -> Result<bool> {
    let pe = Pe::parse(bytes)?;
    let entry = pe.opt_off + pe.data_dir_off + 4 * 8;
    if entry + 8 > bytes.len() {
        return Err("PE data directory is truncated".into());
    }
    if u32at(bytes, entry) == 0 && u32at(bytes, entry + 4) == 0 {
        return Ok(true);
    }
    bytes[entry..entry + 8].fill(0);
    Ok(true)
}

pub fn find_bytes(data: &[u8], pat: &[u8], start: i64) -> i64 {
    if pat.is_empty() {
        return -1;
    }
    let start = start.max(0) as usize;
    let last = data.len().checked_sub(pat.len());
    let Some(last) = last else { return -1 };
    (start..=last)
        .find(|&i| &data[i..i + pat.len()] == pat)
        .map(|i| i as i64)
        .unwrap_or(-1)
}

pub fn match_bytes(data: &[u8], off: i64, pat: &[u8]) -> bool {
    if pat.is_empty() || off < 0 || off as usize + pat.len() > data.len() {
        return false;
    }
    &data[off as usize..off as usize + pat.len()] == pat
}

pub fn match_masked(data: &[u8], off: i64, pat: &[u8], mask: &[u8]) -> bool {
    if pat.is_empty() || pat.len() != mask.len() || off < 0 || off as usize + pat.len() > data.len()
    {
        return false;
    }
    let o = off as usize;
    (0..pat.len()).all(|j| mask[j] == 0 || data[o + j] == pat[j])
}

pub fn find_masked(data: &[u8], pat: &[u8], mask: &[u8], start: i64, length: i64) -> i64 {
    if pat.is_empty() || pat.len() != mask.len() {
        return -1;
    }
    let start = start.max(0);
    if start >= data.len() as i64 || length <= 0 {
        return -1;
    }
    let end = (data.len() as i64).min(start + length);
    let limit = end - pat.len() as i64;
    let mut i = start;
    while i <= limit {
        let o = i as usize;
        if (0..pat.len()).all(|j| mask[j] == 0 || data[o + j] == pat[j]) {
            return i;
        }
        i += 1;
    }
    -1
}

fn off_to_rva(off: i64, pe: &Pe) -> i64 {
    pe.rva_from_off(off).unwrap_or(-1)
}
fn rva_to_off(rva: i64, pe: &Pe) -> i64 {
    for s in &pe.sections {
        let rel = rva - s.vaddr;
        if rel >= 0 && rel < s.rsize {
            return s.rptr + rel;
        }
    }
    -1
}

pub fn find_rip_lea_refs(
    bytes: &[u8],
    start: i64,
    length: i64,
    target_rva: i64,
    pe: &Pe,
) -> Vec<i64> {
    let mut out = Vec::new();
    let start = start.max(0);
    let end = (bytes.len() as i64).min(start + length);
    let mut p = start;
    while p + 7 <= end {
        let i = p as usize;
        if (bytes[i] & 0xF8) == 0x48 && bytes[i + 1] == 0x8D && (bytes[i + 2] & 0xC7) == 0x05 {
            let next = off_to_rva(p + 7, pe);
            if next >= 0 && next + i32le(bytes, i + 3) as i64 == target_rva {
                out.push(p);
            }
        }
        p += 1;
    }
    out
}

pub fn find_xref_arm64(
    bytes: &[u8],
    string_rva: u64,
    section_rva: i64,
    raw_ptr: u32,
    size: u32,
) -> Vec<i64> {
    let mut out = Vec::new();
    let mut i: u32 = 0;
    while i < size {
        let file_off = raw_ptr as u64 + i as u64;
        if i as u64 + 8 > size as u64 || file_off + 8 > bytes.len() as u64 {
            break;
        }
        let fo = file_off as usize;
        let inst1 = u32le(bytes, fo);
        if (inst1 & 0x9F000000) == 0x90000000 {
            let rd = inst1 & 0x1F;
            let imm_lo = ((inst1 >> 29) & 3) as i64;
            let imm_hi = ((inst1 >> 5) & 0x7FFFF) as i64;
            let mut imm = (imm_hi << 2) | imm_lo;
            if imm & 0x100000 != 0 {
                imm |= 0xFFFFFFFFFFE00000u64 as i64;
            }
            imm <<= 12;
            let pc = section_rva as u64 + i as u64;
            let pc_page = pc & 0xFFFFFFFFFFFFF000;
            let page = (pc_page as i64 + imm) as u64;
            let inst2 = u32le(bytes, fo + 4);
            if (inst2 & 0xFFC00000) == 0x91000000 {
                let rn = (inst2 >> 5) & 0x1F;
                if rn == rd {
                    let imm12 = ((inst2 >> 10) & 0xFFF) as u64;
                    if page + imm12 == string_rva {
                        out.push(file_off as i64);
                    }
                }
            }
        }
        i += 4;
    }
    out
}

pub fn find_function_range(
    bytes: &[u8],
    rf_ptr: i64,
    rf_size: i64,
    rva: i64,
    code_rva: i64,
    code_size: i64,
) -> Option<(i64, i64)> {
    if rf_ptr < 0 || rf_size <= 0 || code_size <= 0 {
        return None;
    }
    let rf_end = (bytes.len() as i64).min(rf_ptr + rf_size);
    let code_end = code_rva + code_size;
    let (mut best_b, mut best_f) = (-1i64, -1i64);
    let mut p = rf_ptr;
    while p + 12 <= rf_end {
        let begin = u32at(bytes, p as usize) as i64;
        let finish = u32at(bytes, p as usize + 4) as i64;
        if begin >= code_rva
            && finish > begin
            && finish <= code_end
            && rva >= begin
            && rva < finish
            && (best_b < 0 || begin > best_b || (begin == best_b && finish < best_f))
        {
            best_b = begin;
            best_f = finish;
        }
        p += 12;
    }
    if best_b < 0 {
        None
    } else {
        Some((best_b, best_f))
    }
}

pub fn find_arm64_function_range(
    bytes: &[u8],
    rf_ptr: i64,
    rf_size: i64,
    rva: i64,
    code_rva: i64,
    code_size: i64,
    pe: &Pe,
) -> Option<(i64, i64)> {
    if rf_ptr < 0 || rf_size <= 0 || code_size <= 0 {
        return None;
    }
    let rf_end = (bytes.len() as i64).min(rf_ptr + rf_size);
    let code_end = code_rva + code_size;
    let (mut best_b, mut best_f) = (-1i64, -1i64);
    let mut p = rf_ptr;
    while p + 8 <= rf_end {
        let begin = u32at(bytes, p as usize) as i64;
        let unwind = u32at(bytes, p as usize + 4);
        if begin < code_rva || begin >= code_end || unwind == 0 {
            p += 8;
            continue;
        }
        let flag = unwind & 3;
        let length: i64 = if flag == 1 || flag == 2 {
            (((unwind >> 2) & 0x7FF) * 4) as i64
        } else if flag == 0 {
            let uo = rva_to_off(unwind as i64, pe);
            if uo < 0 || uo > bytes.len() as i64 - 4 {
                p += 8;
                continue;
            }
            let header = u32at(bytes, uo as usize);
            if (header >> 18) & 3 != 0 {
                p += 8;
                continue;
            }
            ((header & 0x3FFFF) * 4) as i64
        } else {
            p += 8;
            continue;
        };
        let finish = begin + length;
        if length > 0
            && finish <= code_end
            && rva >= begin
            && rva < finish
            && (best_b < 0 || begin > best_b || (begin == best_b && finish < best_f))
        {
            best_b = begin;
            best_f = finish;
        }
        p += 8;
    }
    if best_b < 0 {
        None
    } else {
        Some((best_b, best_f))
    }
}

pub fn find_gate_calls_to_rva(
    bytes: &[u8],
    start: i64,
    length: i64,
    target_rva: i64,
    pe: &Pe,
) -> Vec<i64> {
    let mut out = Vec::new();
    let start = start.max(0);
    let end = (bytes.len() as i64).min(start + length);
    let mut p = start;
    while p + 6 <= end {
        let i = p as usize;
        if bytes[i] == 0xE8 {
            let next = off_to_rva(p + 5, pe);
            if next >= 0 && next + i32le(bytes, i + 1) as i64 == target_rva && bytes[i + 5] == 0x88
            {
                out.push(p);
            }
        }
        p += 1;
    }
    out
}

pub fn find_arm64_block_slots_callers(
    bytes: &[u8],
    start: i64,
    length: i64,
    enum_value: u32,
) -> Vec<i64> {
    let mut out = Vec::new();
    if enum_value > 0xFFFF {
        return out;
    }
    let start = start.max(0);
    let end = (bytes.len() as i64).min(start + length);
    let mov_enum = 0x52800001 | (enum_value << 5);
    let mut p = start;
    while p + 12 <= end {
        let i = p as usize;
        let call = u32le(bytes, i);
        let branch = u32le(bytes, i + 4);
        let load = u32le(bytes, i + 8);
        if (call & 0xFC000000) == 0x94000000
            && (branch & 0xFFF8001F) == 0x37000000
            && load == mov_enum
        {
            out.push(p);
        }
        p += 4;
    }
    out
}

pub fn unique_anchor(
    bytes: &[u8],
    pe: &Pe,
    text: &str,
    null_terminated: bool,
) -> Result<(i64, i64)> {
    let mut anchor = text.as_bytes().to_vec();
    if null_terminated {
        anchor.push(0);
    }
    let off = find_bytes(bytes, &anchor, 0);
    if off < 0 || find_bytes(bytes, &anchor, off + 1) >= 0 {
        return Err(format!("{text} anchor was not found uniquely").into());
    }
    let rva = pe
        .rva_from_off(off)
        .ok_or_else(|| format!("{text} anchor RVA was not found"))?;
    Ok((off, rva))
}

pub fn func_range(bytes: &[u8], pe: &Pe, rva: i64) -> Option<(i64, i64)> {
    match pe.arch {
        Arch::X64 => find_function_range(
            bytes,
            pe.exc_rawptr,
            pe.exc_rawsize,
            rva,
            pe.code.vaddr,
            pe.code.vsize.max(pe.code.rsize),
        ),
        Arch::Arm64 => find_arm64_function_range(
            bytes,
            pe.exc_rawptr,
            pe.exc_rawsize,
            rva,
            pe.code.vaddr,
            pe.code.vsize.max(pe.code.rsize),
            pe,
        ),
    }
}

pub fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|h| u8::from_str_radix(h, 16).expect("hex literal in source"))
        .collect()
}

pub fn dedup(mut v: Vec<i64>) -> Vec<i64> {
    v.sort_unstable();
    v.dedup();
    v
}
