use std::fs;

use crate::bundle::Unpacked;
use crate::cli::{say, Opts};
use crate::platform::{bak, Client};
use crate::Result;

pub fn run(o: &Opts) -> Result<()> {
    let c = Client::find(o.path.as_deref())?;
    c.quit();
    if o.no_theme {
        let ui = Unpacked::open(&c.spa())?;
        if ui.remove_theme()? {
            ui.save(&c.spa())?;
            c.resign()?;
        }
        say("Theme removed.");
        return Ok(());
    }
    if !c.has_backup() {
        return Err("no backup found; reinstall Spotify to get a clean client".into());
    }
    fs::rename(c.spa_bak(), c.spa())?;
    for b in c.bins() {
        if bak(&b).is_file() {
            fs::rename(bak(&b), &b)?;
        }
    }
    c.block_widevine(false)?;
    c.resign()?;
    say("Original Spotify restored.");
    Ok(())
}
