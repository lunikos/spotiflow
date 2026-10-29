use crate::commands;
use crate::Result;

pub const HELP: &str = "\
Usage: spotiflow <command> [options]

Commands:
  install      patch Spotify and apply the theme (re-run after every Spotify update)
  theme        reapply the theme only, e.g. after editing assets/css/theme.css
  uninstall    restore the original client from backup
  reinstall    download a clean supported client (macOS), then install
  status       show what is installed
  update       fetch the newest SpotX-Bash patch set (--assets: write into ./assets)

Options:
  -P <path>          Spotify client path (default: auto-detect)
  --premium          paid Premium account: skip free-tier patches
  --no-exp           don't enable experimental features
  --block-updates    block client auto-updates (macOS)
  --hide-podcasts    hide podcasts and audiobooks on Home
  --no-theme         install: patches only; uninstall: remove only the theme
  --videos           keep music videos and Canvas (off by default to save CPU and data)
  --cache-mb <N>     cap Spotify's disk cache at N MB
";

#[derive(Default, Clone)]
pub struct Opts {
    pub path: Option<String>,
    pub premium: bool,
    pub no_exp: bool,
    pub block_updates: bool,
    pub hide_podcasts: bool,
    pub no_theme: bool,
    pub cache_mb: Option<u32>,
    pub assets: bool,
    pub videos: bool,
}

pub fn run(args: &[String]) -> Result<()> {
    let Some(cmd) = args.first() else {
        print!("{HELP}");
        return Ok(());
    };
    let mut o = Opts::default();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-P" => o.path = Some(it.next().ok_or("-P needs a path")?.clone()),
            "--premium" => o.premium = true,
            "--no-exp" => o.no_exp = true,
            "--block-updates" => o.block_updates = true,
            "--hide-podcasts" => o.hide_podcasts = true,
            "--no-theme" => o.no_theme = true,
            "--assets" => o.assets = true,
            "--videos" => o.videos = true,
            "--cache-mb" => {
                o.cache_mb = Some(
                    it.next()
                        .and_then(|n| n.parse().ok())
                        .ok_or("--cache-mb needs a whole number")?,
                )
            }
            "-h" | "--help" => return help(),
            other => return Err(format!("unknown option {other} (see --help)").into()),
        }
    }
    match cmd.as_str() {
        "install" => commands::install::run(&o),
        "theme" => commands::theme::run(&o),
        "uninstall" => commands::uninstall::run(&o),
        "reinstall" => commands::reinstall::run(&o),
        "status" => commands::status::run(&o),
        "update" => commands::update::run(&o),
        "help" | "-h" | "--help" => help(),
        other => Err(format!("unknown command {other} (see --help)").into()),
    }
}

fn help() -> Result<()> {
    print!("{HELP}");
    Ok(())
}

pub fn say(msg: &str) {
    println!("\x1b[1m==>\x1b[0m {msg}");
}

pub fn warn(msg: &str) {
    eprintln!("\x1b[33mwarning:\x1b[0m {msg}");
}
