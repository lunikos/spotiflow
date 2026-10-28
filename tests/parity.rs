use std::env::var;
use std::fs;
use std::path::Path;
use std::process::Command;

use spotiflow::bundle::snapshot;
use spotiflow::bundle::Unpacked;
use spotiflow::patch::{self, Patch};
use spotiflow::platform::Os;
use spotiflow::ver::Ver;

const GROUPS: &[&str] = &["ao", "na", "free", "exp", "premium", "pod", "updates"];

fn perl(file: &Path, p: &Patch) {
    let prog = format!("s&{}&{}&{}", p.regex, p.replace, p.flags);
    let ok = Command::new("perl")
        .args(["-0777", "-pi", "-e", &prog])
        .arg(file)
        .status()
        .unwrap()
        .success();
    assert!(ok, "perl failed on {}", p.name);
}

fn check(input: &[u8], kind: &str, os: Os, arch: &str, ver: Ver) -> (usize, Vec<&'static str>) {
    let tmp = std::env::temp_dir().join(format!("spotiflow-parity-{kind}"));
    fs::write(&tmp, input).unwrap();
    let (mut ours, mut n, mut missed) = (input.to_vec(), 0, Vec::new());
    for p in patch::table()
        .into_iter()
        .filter(|p| p.file == kind || (kind == "xpuiJs" && p.file == "vendorXpuiJs"))
    {
        if !p.applies(GROUPS, ver, os, arch) {
            continue;
        }
        perl(&tmp, &p);
        let (next, miss) = patch::apply_bytes(&ours, std::slice::from_ref(&p)).unwrap();
        ours = next;
        missed.extend(miss);
        assert!(
            fs::read(&tmp).unwrap() == ours,
            "{kind}: output differs from Perl after patch {}",
            p.name
        );
        n += 1;
    }
    fs::remove_file(tmp).unwrap();
    (n, missed)
}

#[test]
#[ignore]
fn matches_perl_on_real_client() {
    let Ok(spa) = var("PARITY_SPA") else { return };
    let os = match var("PARITY_OS").unwrap().as_str() {
        "macOS" => Os::Mac,
        "Linux" => Os::Linux,
        "Windows" => Os::Windows,
        other => panic!("unknown PARITY_OS {other}"),
    };
    let arch = var("PARITY_ARCH").unwrap();
    let ver = Ver::parse(&var("PARITY_VER").unwrap()).unwrap();
    let ui = Unpacked::open(Path::new(&spa)).unwrap();
    let js = ui.path("xpui-snapshot.js");
    snapshot::extract(Path::new(&var("PARITY_SNAPSHOT").unwrap()), &js).unwrap();

    let mut results = vec![
        check(&fs::read(&js).unwrap(), "xpuiJs", os, &arch, ver),
        check(
            &fs::read(ui.path("xpui-desktop-modals.js")).unwrap(),
            "xpuiDesktopModalsJs",
            os,
            &arch,
            ver,
        ),
    ];
    if let Ok(bin) = var("PARITY_BIN") {
        results.push(check(&fs::read(bin).unwrap(), "appBinary", os, &arch, ver));
    }
    let n: usize = results.iter().map(|r| r.0).sum();
    let missed: Vec<_> = results.into_iter().flat_map(|r| r.1).collect();
    println!(
        "{n} patches identical to Perl; {} matched nothing: {}",
        missed.len(),
        missed.join(", ")
    );
}
