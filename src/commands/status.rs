use std::fs;

use super::MARKER;
use crate::bundle::{Unpacked, THEME_FILE};
use crate::cli::Opts;
use crate::platform::Client;
use crate::Result;

pub fn run(o: &Opts) -> Result<()> {
    let c = Client::find(o.path.as_deref())?;
    let d = crate::upstream::data();
    let ui = Unpacked::open(&c.spa())?;
    let patched = ["xpui-snapshot.js", "xpui.js"]
        .iter()
        .any(|f| fs::read(ui.path(f)).is_ok_and(|b| b.ends_with(MARKER.as_bytes())));
    let yes = |b: bool| if b { "yes" } else { "no" };
    println!("client   {} ({} {})", c.root.display(), c.os.name(), c.arch);
    println!(
        "version  {} (patch set: {}, SpotX-Bash {})",
        c.version.map_or("unknown".into(), |v| v.to_string()),
        d.client,
        &d.commit[..7.min(d.commit.len())]
    );
    println!("patched  {}", yes(patched));
    println!("theme    {}", yes(ui.path(THEME_FILE).is_file()));
    println!("backup   {}", yes(c.has_backup()));
    Ok(())
}
