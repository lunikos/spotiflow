use std::fs;
use std::path::Path;

use crate::Result;

const START: &str = "var __webpack_modules__={";
const END: &str = "xpui-modules.js.map";

pub fn extract(snapshot_bin: &Path, snapshot_js: &Path) -> Result<()> {
    let bin = fs::read(snapshot_bin)
        .map_err(|e| format!("can't read {}: {e}", snapshot_bin.display()))?;
    let code = find(&bin)
        .ok_or("V8 snapshot layout not recognised; this Spotify version isn't supported")?;
    let rest = fs::read_to_string(snapshot_js)?;
    fs::write(snapshot_js, format!("{code}\n{rest}"))?;
    Ok(())
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn find(bin: &[u8]) -> Option<String> {
    let (start, end) = (utf16(START), utf16(END));
    let s = bin
        .windows(start.len())
        .position(|w| w == start.as_slice())?;
    let e = s
        + bin[s..]
            .windows(end.len())
            .position(|w| w == end.as_slice())?
        + end.len();
    let units: Vec<u16> = bin[s..e]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_module_table() {
        let mut bin = vec![0xff, 0xfe, 1, 2, 3];
        bin.extend(utf16(
            "var __webpack_modules__={1:x}//# sourceMappingURL=xpui-modules.js.map",
        ));
        bin.extend([9, 9, 9]);
        assert_eq!(
            find(&bin).unwrap(),
            "var __webpack_modules__={1:x}//# sourceMappingURL=xpui-modules.js.map"
        );
        assert_eq!(find(b"nothing here"), None);
    }
}
