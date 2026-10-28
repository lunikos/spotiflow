use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::{say, Opts};
use crate::platform::{apps_dir, Client, Os};
use crate::upstream;
use crate::Result;

const VERSIONS: &str =
    "https://raw.githubusercontent.com/LoaderSpot/table/main/table/versions.json";
const SPOTIFY_TEAM_ID: &str = "2FNC3A47ZF";

pub fn run(o: &Opts) -> Result<()> {
    let os = Os::current();
    let client = upstream::data().client.trim();
    let short = client.split(".g").next().unwrap_or(client);
    let arm = cfg!(target_arch = "aarch64");
    let (os_key, arch_key) = match os {
        Os::Mac => ("mac", if arm { "arm64" } else { "intel" }),
        Os::Windows => ("win", if arm { "arm64" } else { "x64" }),
        Os::Linux => {
            return Err(
                "on Linux, install Spotify with your package manager, then run install".into(),
            )
        }
    };
    let json = String::from_utf8(output(
        Command::new("curl").args(["-fsSL", VERSIONS]),
        "fetch versions.json",
    )?)?;
    let url = asset_url(&json, short, os_key, arch_key)
        .ok_or(format!("no {os_key} {arch_key} download for {client}"))?;

    let work = std::env::temp_dir().join(format!("spotiflow-dl-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;
    let file = work.join(url.rsplit('/').next().unwrap_or("spotify"));
    say(&format!("Downloading Spotify {client}"));
    output(
        Command::new("curl")
            .args(["-fL", "--progress-bar", "-o"])
            .arg(&file)
            .arg(&url),
        "download",
    )?;

    let path = match os {
        Os::Mac => install_mac(&file, &work, o.path.as_deref())?,
        _ => install_windows(&file, &work, o.path.as_deref())?,
    };
    let _ = fs::remove_dir_all(&work);
    super::install::run(&Opts { path, ..o.clone() })
}

fn install_mac(tbz: &Path, work: &Path, dest: Option<&str>) -> Result<Option<String>> {
    let dest = PathBuf::from(dest.unwrap_or("/Applications/Spotify.app"));
    let staged = work.join("Spotify.app");
    fs::create_dir_all(&staged)?;
    output(
        Command::new("tar")
            .arg("-xpf")
            .arg(tbz)
            .arg("-C")
            .arg(&staged),
        "unpack",
    )?;
    if !apps_dir(&staged).join("xpui.spa").is_file() {
        return Err("the download isn't a Spotify client".into());
    }
    output(
        Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&staged),
        "signature check",
    )?;
    let info = Command::new("codesign")
        .args(["-dv"])
        .arg(&staged)
        .output()?;
    if !String::from_utf8_lossy(&info.stderr).contains(&format!("TeamIdentifier={SPOTIFY_TEAM_ID}"))
    {
        return Err("the download isn't signed by Spotify; not installing it".into());
    }
    swap(&staged, &dest, &work.join("Spotify.old.app"))
}

fn install_windows(exe: &Path, work: &Path, dest: Option<&str>) -> Result<Option<String>> {
    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
    let dest = dest.map_or_else(|| PathBuf::from(appdata).join("Spotify"), PathBuf::from);
    let staged = work.join("client");
    output(
        Command::new(exe).arg("/extract").arg(&staged),
        "Spotify installer",
    )?;
    if !apps_dir(&staged).join("xpui.spa").is_file() {
        return Err("the installer didn't produce a Spotify client".into());
    }
    swap(&staged, &dest, &work.join("old"))
}

fn swap(staged: &Path, dest: &Path, old: &Path) -> Result<Option<String>> {
    if let Ok(c) = Client::find(Some(&dest.to_string_lossy())) {
        c.quit();
    }
    if dest.exists() {
        fs::rename(dest, old).map_err(|e| format!("couldn't move the old client aside: {e}"))?;
    }
    if let Err(e) = fs::rename(staged, dest) {
        let _ = fs::rename(old, dest);
        return Err(
            format!("couldn't install the new client ({e}); the old one was put back").into(),
        );
    }
    say(&format!("Installed {}", dest.display()));
    Ok(Some(dest.to_string_lossy().into_owned()))
}

fn output(cmd: &mut Command, what: &str) -> Result<Vec<u8>> {
    let out = cmd.output()?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!(
            "{what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into())
    }
}

fn asset_url(json: &str, version: &str, os: &str, arch: &str) -> Option<String> {
    let entry = &json[json.find(&format!("\"{version}\""))?..];
    let own = entry.find("\"fullversion\"")? + 1;
    let entry = &entry[..entry[own..]
        .find("\"fullversion\"")
        .map_or(entry.len(), |i| i + own)];
    let platform = &entry[entry.find(&format!("\"{os}\""))?..];
    let asset = &platform[platform.find(&format!("\"{arch}\""))?..];
    let url = &asset[asset.find("\"url\"")? + 5..];
    let start = url.find('"')? + 1;
    Some(url[start..start + url[start..].find('"')?].to_string())
}

#[cfg(test)]
mod tests {
    use super::asset_url;

    #[test]
    fn finds_asset() {
        let json = r#"{"1.3.6.2":{"fullversion":"x","mac":{"arm64":{"url":"A2"}}},"1.3.6.1":{"fullversion":"y","mac":{"arm64":{"url":"A1","size":1},"intel":{"url":"I1"}},"win":{"x64":{"url":"W1"}}}}"#;
        assert_eq!(
            asset_url(json, "1.3.6.1", "mac", "intel").as_deref(),
            Some("I1")
        );
        assert_eq!(
            asset_url(json, "1.3.6.1", "win", "x64").as_deref(),
            Some("W1")
        );
        assert_eq!(asset_url(json, "1.3.6.2", "win", "x64"), None);
    }
}
