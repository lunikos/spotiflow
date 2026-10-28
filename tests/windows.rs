//! Checks the Rust Windows binary patcher against SpotX's own output, byte for byte, on real
//! Spotify.dll/.exe/chrome_elf.dll (1.3.6.1921). Each env var points at a directory holding the
//! three pristine PE files:
//!   SPOTIFLOW_WIN_X64=/path/x64 SPOTIFLOW_WIN_ARM64=/path/arm64 \
//!     cargo test --release --test windows -- --ignored
//! The expected hashes were produced by running SpotX's run.ps1 patch functions under PowerShell.

use std::fs;
use std::path::Path;

use spotiflow::patch::windows::patch_windows;
use spotiflow::ver::Ver;

const VER: Ver = Ver([1, 3, 6, 1921]);

// (pristine env dir, expected non-premium dll/exe/chrome, expected premium dll)
struct Case {
    env: &'static str,
    dll: &'static str,
    exe: &'static str,
    chrome: &'static str,
    dll_premium: &'static str,
}

const CASES: &[Case] = &[
    Case {
        env: "SPOTIFLOW_WIN_X64",
        dll: "1eec8f8a5d028df6511b901b321fbf8cb6eb384b5140d97f0e8589b40d9295d6",
        exe: "dfb7c49c62a75904ddac54a2e7fcca2ed83ff8a040fda4e6a77d54294b7891a9",
        chrome: "287a3bccbc890358b92a8e603212dae1c520ba9a3a57c7e46156bd8b14d191cb",
        dll_premium: "f781037360ebc3dfc7ab1e90284d8d9668156cee096e41f43f2323779184d701",
    },
    Case {
        env: "SPOTIFLOW_WIN_ARM64",
        dll: "b7a277fb2a99bbb498b0d40436a7529ed784638f281fa29240327592dcdfea63",
        exe: "54a4733b216151745b96d70a80dbbec6137fbeff67fafe41ce3598ce1a4f5dfb",
        chrome: "b379b241e180eb9e6079c8aab2643b90fe7cdcdcefcdd3605691246d70838181",
        dll_premium: "31cb94eac73f2cb3af4f846b559c7b09cb5dd37278191b6556078ef5f0e9f0c6",
    },
];

fn stage(src: &Path, tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("spotiflow-win-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for f in ["Spotify.dll", "Spotify.exe", "chrome_elf.dll"] {
        fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    dir
}

fn hashes(dir: &Path) -> (String, String, String) {
    let h = |f: &str| sha256(&fs::read(dir.join(f)).unwrap());
    (h("Spotify.dll"), h("Spotify.exe"), h("chrome_elf.dll"))
}

#[test]
#[ignore]
fn matches_spotx() {
    let mut ran = 0;
    for c in CASES {
        let Ok(src) = std::env::var(c.env) else { continue };
        let src = Path::new(&src);

        // Non-premium: all patches.
        let dir = stage(src, "free");
        let missed = patch_windows(&dir, VER, false, true).unwrap();
        assert!(missed.is_empty(), "{}: unexpected misses {missed:?}", c.env);
        let got = hashes(&dir);
        assert_eq!(
            got,
            (c.dll.into(), c.exe.into(), c.chrome.into()),
            "{} non-premium output differs from SpotX",
            c.env
        );

        // Re-running on the patched client must be a no-op (SpotX "Patched" state).
        patch_windows(&dir, VER, false, true).unwrap();
        assert_eq!(hashes(&dir), got, "{} re-run changed bytes", c.env);
        fs::remove_dir_all(&dir).unwrap();

        // Premium: skips block_slots and crossfade; signatures and regex still applied.
        let dir = stage(src, "premium");
        patch_windows(&dir, VER, true, true).unwrap();
        let (dll, exe, chrome) = hashes(&dir);
        assert_eq!(dll, c.dll_premium, "{} premium Spotify.dll differs from SpotX", c.env);
        assert_eq!((exe, chrome), (c.exe.into(), c.chrome.into()), "{} premium exe/chrome differ", c.env);
        fs::remove_dir_all(&dir).unwrap();

        ran += 1;
    }
    println!("{ran} architecture(s) matched SpotX exactly");
}

// Minimal SHA-256 (self-contained; avoids a dependency for the parity check).
fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for ci in (0..msg.len()).step_by(64) {
        let chunk = &msg[ci..ci + 64];
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}
