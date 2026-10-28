pub mod snapshot;

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Result;

pub const THEME_FILE: &str = "spotiflow.css";
const THEME_LINK: &str = r#"<link rel="stylesheet" href="/spotiflow.css">"#;

pub struct Unpacked {
    pub dir: PathBuf,
}

impl Drop for Unpacked {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Unpacked {
    pub fn open(spa: &Path) -> Result<Unpacked> {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("spotiflow-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        let out = Unpacked { dir };
        let mut zip = ZipArchive::new(fs::File::open(spa)?)
            .map_err(|e| format!("can't read {}: {e}", spa.display()))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(rel) = entry.enclosed_name() else {
                continue;
            };
            let path = out.dir.join(rel);
            if entry.is_dir() {
                fs::create_dir_all(&path)?;
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            fs::write(path, buf)?;
        }
        Ok(out)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn save(&self, spa: &Path) -> Result<()> {
        let tmp = spa.with_extension("spa.tmp");
        {
            let mut zip = ZipWriter::new(fs::File::create(&tmp)?);
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let mut files = Vec::new();
            walk(&self.dir, &mut files)?;
            files.sort();
            for f in files {
                let name = f
                    .strip_prefix(&self.dir)?
                    .to_string_lossy()
                    .replace('\\', "/");
                zip.start_file(name, opts)?;
                zip.write_all(&fs::read(&f)?)?;
            }
            zip.finish()?;
        }
        let mut check = ZipArchive::new(fs::File::open(&tmp)?)?;
        for i in 0..check.len() {
            std::io::copy(&mut check.by_index(i)?, &mut std::io::sink())?;
        }
        fs::rename(&tmp, spa)?;
        Ok(())
    }

    pub fn add_theme(&self, css: &str) -> Result<()> {
        fs::write(self.path(THEME_FILE), css)?;
        let index = self.path("index.html");
        let html = fs::read_to_string(&index)?;
        if !html.contains(THEME_LINK) {
            let at = html.find("</head>").ok_or("index.html has no </head>")?;
            fs::write(
                &index,
                format!("{}{THEME_LINK}{}", &html[..at], &html[at..]),
            )?;
        }
        Ok(())
    }

    pub fn remove_theme(&self) -> Result<bool> {
        let index = self.path("index.html");
        let html = fs::read_to_string(&index)?;
        let had = self.path(THEME_FILE).is_file() || html.contains(THEME_LINK);
        let _ = fs::remove_file(self.path(THEME_FILE));
        fs::write(&index, html.replace(THEME_LINK, ""))?;
        Ok(had)
    }

    pub fn append(&self, name: &str, text: &str) -> Result<()> {
        let mut f = fs::OpenOptions::new().append(true).open(self.path(name))?;
        write!(f, "\n{text}")?;
        Ok(())
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for e in fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(&p, out)?
        } else {
            out.push(p)
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_spa(path: &Path) {
        let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
        zip.start_file("index.html", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"<html><head><link href="/a.css" rel="stylesheet"></head></html>"#)
            .unwrap();
        zip.add_directory("sub/", SimpleFileOptions::default())
            .unwrap();
        zip.start_file("sub/x.js", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn theme_round_trip() {
        let spa = std::env::temp_dir().join(format!("spotiflow-test-{}.spa", std::process::id()));
        fake_spa(&spa);
        for _ in 0..2 {
            let u = Unpacked::open(&spa).unwrap();
            u.add_theme("a{}").unwrap();
            u.save(&spa).unwrap();
        }
        let u = Unpacked::open(&spa).unwrap();
        let html = fs::read_to_string(u.path("index.html")).unwrap();
        assert_eq!(html.matches(THEME_LINK).count(), 1);
        assert!(html.contains(&format!(r#"rel="stylesheet">{THEME_LINK}</head>"#)));
        assert_eq!(fs::read_to_string(u.path("sub/x.js")).unwrap(), "x");
        assert!(u.remove_theme().unwrap());
        u.save(&spa).unwrap();
        drop(u);
        let u = Unpacked::open(&spa).unwrap();
        assert!(!u.path(THEME_FILE).exists());
        assert!(!fs::read_to_string(u.path("index.html"))
            .unwrap()
            .contains("spotiflow"));
        drop(u);
        fs::remove_file(spa).unwrap();
    }
}
