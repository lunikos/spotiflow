use super::THEME;
use crate::bundle::Unpacked;
use crate::cli::{say, Opts};
use crate::platform::Client;
use crate::Result;

pub fn run(o: &Opts) -> Result<()> {
    let c = Client::find(o.path.as_deref())?;
    c.quit();
    let ui = Unpacked::open(&c.spa())?;
    ui.add_theme(THEME)?;
    ui.save(&c.spa())?;
    c.resign()?;
    say("Theme applied. Start Spotify.");
    Ok(())
}
