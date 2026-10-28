use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::install::MIN;
use crate::cli::{say, Opts};
use crate::upstream::{self, spotx_bash, FILES};
use crate::{patch, Result};

const REPO: &str = "SpotX-Official/SpotX-Bash";

pub fn run(o: &Opts) -> Result<()> {
    let api = fetch(&format!("https://api.github.com/repos/{REPO}/commits/main"))?;
    let sha = api
        .split("\"sha\":")
        .nth(1)
        .and_then(|s| s.split('"').nth(1))
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("unexpected GitHub API reply")?;
    let current = upstream::data();
    if sha == current.commit && !o.assets {
        say(&format!(
            "Already on SpotX-Bash {} (client {})",
            &sha[..7],
            current.client
        ));
        return Ok(());
    }
    let script = fetch(&format!(
        "https://raw.githubusercontent.com/{REPO}/{sha}/spotx.sh"
    ))?;
    let new = spotx_bash::extract(&script, MIN)?;
    let count = patch::validate(Box::leak(new.patches.clone().into_boxed_str()))?;

    let source = format!("{sha} {}\n", new.client);
    let base = if o.assets {
        PathBuf::from("assets")
    } else {
        upstream::cache_dir()
    };
    for (file, text) in FILES
        .iter()
        .zip([&new.patches, &new.free_css, &new.podcasts_css, &source])
    {
        let path = base.join(file);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, text)?;
    }
    say(&format!(
        "SpotX-Bash {}: {count} patches for clients up to {}",
        &sha[..7],
        new.client
    ));
    Ok(())
}

fn fetch(url: &str) -> Result<String> {
    let out = Command::new("curl")
        .args(["-fsSL", "-H", "User-Agent: spotiflow", url])
        .output()?;
    if !out.status.success() {
        return Err(format!("download failed: {url}").into());
    }
    Ok(String::from_utf8(out.stdout)?)
}
