mod linux;
mod macos;
mod windows;

use std::path::PathBuf;

use crate::ver::Ver;
use crate::Result;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Os {
    Mac,
    Linux,
    Windows,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Os::Mac => "macOS",
            Os::Linux => "Linux",
            Os::Windows => "Windows",
        }
    }
}

pub struct Client {
    pub os: Os,
    pub root: PathBuf,
    pub apps: PathBuf,
    pub bin: PathBuf,
    pub snapshot: PathBuf,
    pub arch: &'static str,
    pub version: Option<Ver>,
}

impl Client {
    pub fn find(path: Option<&str>) -> Result<Client> {
        let os = Os::current();
        let candidates = match path {
            Some(p) => vec![expand_home(p)],
            None => match os {
                Os::Mac => macos::candidates(),
                Os::Linux => linux::candidates(),
                Os::Windows => windows::candidates(),
            },
        };
        let root = candidates
            .into_iter()
            .find(|c| apps_dir(c).join("xpui.spa").is_file() || apps_dir(c).join("xpui").is_dir())
            .ok_or("Spotify not found; pass its path with -P")?;
        let apps = apps_dir(&root);
        let (bin, snapshot, arch, version) = match os {
            Os::Mac => macos::layout(&root),
            Os::Linux => linux::layout(&root),
            Os::Windows => windows::layout(&root),
        };
        Ok(Client {
            os,
            root,
            apps,
            bin,
            snapshot,
            arch,
            version,
        })
    }

    pub fn spa(&self) -> PathBuf {
        self.apps.join("xpui.spa")
    }
    pub fn spa_bak(&self) -> PathBuf {
        self.apps.join("xpui.bak")
    }
    pub fn bin_bak(&self) -> PathBuf {
        bak(&self.bin)
    }

    pub fn bins(&self) -> Vec<PathBuf> {
        if self.os == Os::Windows {
            ["Spotify.exe", "Spotify.dll", "chrome_elf.dll"]
                .iter()
                .map(|f| self.root.join(f))
                .collect()
        } else {
            vec![self.bin.clone()]
        }
    }

    pub fn has_backup(&self) -> bool {
        self.spa_bak().is_file() && self.bins().iter().all(|b| bak(b).is_file())
    }

    pub fn quit(&self) {
        match self.os {
            Os::Mac => macos::quit(),
            Os::Linux => linux::quit(),
            Os::Windows => windows::quit(),
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    pub fn resign(&self) -> Result<()> {
        if self.os == Os::Mac {
            macos::resign(&self.root)
        } else {
            Ok(())
        }
    }

    pub fn prefs(&self) -> PathBuf {
        match self.os {
            Os::Mac => home().join("Library/Application Support/Spotify/prefs"),
            Os::Linux => home().join(".config/spotify/prefs"),
            Os::Windows => self.root.join("prefs"),
        }
    }
}

pub fn apps_dir(root: &std::path::Path) -> PathBuf {
    if root.join("Contents").is_dir() {
        root.join("Contents/Resources/Apps")
    } else {
        root.join("Apps")
    }
}

pub fn home() -> PathBuf {
    PathBuf::from(
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .unwrap_or_default(),
    )
}

pub fn bak(p: &std::path::Path) -> PathBuf {
    let mut name = p.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    p.with_file_name(name)
}

fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

fn host_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

fn run_quietly(cmd: &str, args: &[&str]) {
    let _ = std::process::Command::new(cmd).args(args).output();
}
