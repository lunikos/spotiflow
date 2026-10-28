use std::fs;
use std::process::Command;

use super::{MARKER, THEME};
use crate::bundle::snapshot;
use crate::bundle::Unpacked;
use crate::cli::{say, warn, Opts};
use crate::patch::Group;
use crate::platform::{bak, Client, Os};
use crate::ver::Ver;
use crate::{patch, upstream, Result};

pub const MIN: Ver = Ver([1, 3, 0, 0]);

pub fn run(o: &Opts) -> Result<()> {
    let c = Client::find(o.path.as_deref())?;
    say(&format!(
        "Spotify at {} ({} {})",
        c.root.display(),
        c.os.name(),
        c.arch
    ));
    c.quit();
    undo_spicetify(&c)?;
    fresh_base(&c)?;

    let ui = Unpacked::open(&c.spa())?;
    let (js, css) = if ui.path("xpui.js").is_file() {
        ("xpui.js", "xpui.css")
    } else {
        snapshot::extract(&c.snapshot, &ui.path("xpui-snapshot.js"))?;
        ("xpui-snapshot.js", "xpui-snapshot.css")
    };
    let ver = c
        .version
        .or_else(|| version_from_js(&ui.path(js)))
        .ok_or("can't read the Spotify version")?;
    if ver < MIN {
        return Err(format!(
            "Spotify {ver} is older than {MIN}; update it or run: spotiflow reinstall"
        )
        .into());
    }
    let supported = upstream::data().supported();
    if ver > supported {
        warn(&format!("Spotify {ver} is newer than {supported}, the newest the patch set supports; run: spotiflow update"));
    }

    let groups: Vec<Group> = [
        (Group::Ads, true),
        (Group::NoAds, true),
        (Group::Free, !o.premium),
        (Group::Experiments, !o.no_exp),
        (Group::PremiumExperiments, o.premium && !o.no_exp),
        (Group::Podcasts, o.hide_podcasts),
        (Group::Updates, o.block_updates),
    ]
    .into_iter()
    .filter_map(|(g, on)| on.then_some(g))
    .collect();
    say(&format!("Patching Spotify {ver} ({groups:?})"));
    let missed = patch::apply(
        &c,
        &groups,
        ver,
        &ui.path(js),
        &ui.path("xpui-desktop-modals.js"),
    )?;
    if !missed.is_empty() {
        warn(&format!(
            "{} patches matched nothing: {}",
            missed.len(),
            missed.join(", ")
        ));
    }
    if !o.premium {
        ui.append(css, upstream::data().free_css)?;
    }
    if o.hide_podcasts {
        ui.append(css, upstream::data().podcasts_css)?;
    }
    ui.append(js, MARKER)?;
    if !o.no_theme {
        say("Adding theme");
        ui.add_theme(THEME)?;
    }
    ui.save(&c.spa())?;
    c.resign()?;
    if c.os == Os::Windows {
        let wm = patch::windows::patch_windows(&c.root, ver, o.premium, o.block_updates)?;
        if !wm.is_empty() {
            warn(&format!(
                "{} binary patches matched nothing: {}",
                wm.len(),
                wm.join(", ")
            ));
        }
    }
    if let Some(mb) = o.cache_mb {
        cap_cache(&c, mb)?;
    }
    say("Done. Start Spotify.");
    Ok(())
}

/// Every install starts from the untouched client: restore backups if present, else make them.
fn fresh_base(c: &Client) -> Result<()> {
    if c.has_backup() {
        fs::copy(c.spa_bak(), c.spa())?;
        for b in c.bins() {
            fs::copy(bak(&b), &b)?;
        }
        return Ok(());
    }
    if is_patched(c)? {
        return Err("Spotify is already patched (SpotX or spotiflow) but has no backup; reinstall Spotify (macOS: spotiflow reinstall)".into());
    }
    fs::copy(c.spa(), c.spa_bak())?;
    for b in c.bins() {
        fs::copy(&b, bak(&b))?;
    }
    Ok(())
}

fn is_patched(c: &Client) -> Result<bool> {
    let ui = Unpacked::open(&c.spa())?;
    let marked = |f: &str| {
        fs::read(ui.path(f))
            .is_ok_and(|b| contains(&b, MARKER.as_bytes()) || contains(&b, b"SpotX"))
    };
    Ok(marked("xpui-snapshot.js") || marked("xpui.js") || marked("xpui-desktop-modals.js"))
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Spicetify unpacks xpui.spa into Apps/xpui and drops other files; restore it, keeping our backup.
fn undo_spicetify(c: &Client) -> Result<()> {
    if !c.apps.join("xpui").is_dir() {
        return Ok(());
    }
    say("Spicetify is applied; running 'spicetify restore'");
    let keep = fs::read(c.spa_bak()).ok();
    let ok = Command::new("spicetify")
        .arg("restore")
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok || !c.spa().is_file() {
        return Err(
            "Spotify is unpacked by spicetify and 'spicetify restore' failed; reinstall Spotify"
                .into(),
        );
    }
    if let (Some(bak), false) = (keep, c.spa_bak().is_file()) {
        fs::write(c.spa_bak(), bak)?;
    }
    Ok(())
}

/// Same lookup SpotX-Bash falls back to when the OS doesn't expose a version.
fn version_from_js(js: &std::path::Path) -> Option<Ver> {
    let text = String::from_utf8_lossy(&fs::read(js).ok()?).into_owned();
    let re =
        fancy_regex::Regex::new(r#"[Vv]ersion[:=,"]{1,3}(1\.[0-9]+\.[0-9]+\.[0-9]+)\.g[0-9a-f]+"#)
            .ok()?;
    let caps = re.captures(&text).ok()??;
    Ver::parse(caps.get(1)?.as_str())
}

/// Spotify reads storage.size (MB) from prefs and rewrites the file on exit, so it must be closed.
fn cap_cache(c: &Client, mb: u32) -> Result<()> {
    let prefs = c.prefs();
    let Ok(text) = fs::read_to_string(&prefs) else {
        warn("no prefs file yet (start Spotify once); cache cap skipped");
        return Ok(());
    };
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.starts_with("storage.size="))
        .map(String::from)
        .collect();
    lines.push(format!("storage.size={mb}"));
    fs::write(&prefs, lines.join("\n") + "\n")?;
    say(&format!("Cache capped at {mb} MB"));
    Ok(())
}
