pub mod spotx_bash;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::platform::home;
use crate::ver::Ver;

pub const FILES: [&str; 4] = [
    "patches/spotx.txt",
    "css/free.css",
    "css/podcasts.css",
    "patches/source.txt",
];

pub struct Data {
    pub patches: &'static str,
    pub free_css: &'static str,
    pub podcasts_css: &'static str,
    pub commit: &'static str,
    pub client: &'static str,
}

impl Data {
    fn new([patches, free_css, podcasts_css, source]: [&'static str; 4]) -> Option<Data> {
        let (commit, client) = source.trim().split_once(' ')?;
        Some(Data {
            patches,
            free_css,
            podcasts_css,
            commit,
            client,
        })
    }

    pub fn supported(&self) -> Ver {
        Ver::parse(self.client).unwrap_or(Ver([0; 4]))
    }
}

pub fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| {
        let embedded = Data::new([
            include_str!("../../assets/patches/spotx.txt"),
            include_str!("../../assets/css/free.css"),
            include_str!("../../assets/css/podcasts.css"),
            include_str!("../../assets/patches/source.txt"),
        ])
        .expect("assets/patches/source.txt holds '<commit> <client>'");
        load(&cache_dir())
            .filter(|c| c.supported() >= embedded.supported())
            .unwrap_or(embedded)
    })
}

fn load(dir: &Path) -> Option<Data> {
    let mut files = [""; 4];
    for (slot, f) in files.iter_mut().zip(FILES) {
        *slot = Box::leak(fs::read_to_string(dir.join(f)).ok()?.into_boxed_str());
    }
    Data::new(files)
}

pub fn cache_dir() -> PathBuf {
    let var = |k| std::env::var_os(k).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home().join("Library/Application Support/spotiflow")
    } else if cfg!(windows) {
        var("APPDATA").unwrap_or_else(home).join("spotiflow")
    } else {
        var("XDG_CONFIG_HOME")
            .unwrap_or_else(|| home().join(".config"))
            .join("spotiflow")
    }
}
