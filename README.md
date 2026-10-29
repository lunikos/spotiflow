<p align="center">
  <img src="assets/logo.svg" width="96" alt="">
</p>

<h1 align="center">spotiflow</h1>

<p align="center">
  Ad-free, decluttered and lighter Spotify desktop app for macOS, Linux and Windows.<br>
  Built on <a href="https://github.com/SpotX-Official/SpotX">SpotX</a>, with its own theme and CPU fixes.
</p>

## Get started

```sh
curl -fsSL github.com/lunikos/spotiflow/raw/main/spotiflow.cmd | sh -s install   # macOS, Linux (sudo sh)
```

```powershell
curl.exe -fsSLO github.com/lunikos/spotiflow/raw/main/spotiflow.cmd; .\spotiflow.cmd install   # Windows
```

Run it again after each Spotify update, with `uninstall` in place of `install` to undo.

<details>
<summary><b>Supported installs</b></summary>

| Platform | Spotify it works with                                                         |
| -------- | ----------------------------------------------------------------------------- |
| macOS    | `/Applications/Spotify.app` or `~/Applications/Spotify.app`                   |
| Windows  | The desktop installer's `%APPDATA%\Spotify` (not the Microsoft Store version) |
| Linux    | The `.deb` or spotify-launcher install. Run with `sudo`. Flatpak and Snap are read-only, so they can't be patched |

</details>

## Commands

| Command     | What it does                                                       |
| ----------- | ------------------------------------------------------------------ |
| `install`   | Patch Spotify and apply the theme                                  |
| `theme`     | Re-apply only the theme, e.g. after editing `assets/css/theme.css` |
| `uninstall` | Put the original Spotify back                                      |
| `reinstall` | Download a clean Spotify and install over it (macOS, Windows)      |
| `update`    | Pull the latest SpotX patch set                                    |
| `status`    | Show what's installed                                              |

<details>
<summary><b>Options</b></summary>

| Option            | Use it to                                     |
| ----------------- | --------------------------------------------- |
| `--premium`       | Skip the ad patches on a paid account         |
| `--no-theme`      | Patch only (with `uninstall`: drop the theme) |
| `--no-exp`        | Leave experimental features off               |
| `--hide-podcasts` | Hide podcasts and audiobooks on Home          |
| `--block-updates` | Stop Spotify updating itself (macOS, Windows) |
| `--videos`        | Keep music videos and Canvas (off by default)  |
| `--cache-mb <N>`  | Cap Spotify's disk cache at N MB              |
| `-P <path>`       | Point at a Spotify install it can't find      |

</details>

## Staying up to date

- `spotiflow update` pulls the newest SpotX-Bash patch set and keeps it only if every pattern
  compiles. It trusts upstream SpotX as much as running SpotX would.
- A weekly workflow does the same for this repo and opens a pull request.

<details>
<summary><b>How it works</b></summary>

1. Find Spotify, close it, and back it up the first time (or restore that backup on later runs).
2. Unpack `xpui.spa`, Spotify's web UI. Newer builds hide part of their code in the V8 snapshot,
   so that's copied out first.
3. Apply the patches that match your version, OS and CPU. On Windows, also patch `Spotify.dll`
   and strip the signatures SpotX strips from `Spotify.exe` and `chrome_elf.dll`.
4. Add a small script that pauses Spotify's scroll-linked animations when nothing scrolls. They
   otherwise keep the compositor drawing every frame (about 30% CPU while idle).
   Spotify's 33 fps "now playing" equaliser GIF is also swapped for an 11 fps copy.
   Music videos and Canvas are switched off: decoding them costs about 50% CPU and extra data.
   Spotify's Widevine video module is then kept from loading, saving about 100 MB of RAM.
   Diagnostic telemetry (performance timings, A/B experiment reports, telemetry self-stats) is
   dropped.
5. Add the theme stylesheet, repack, and re-sign the app on macOS.

The patch records keep SpotX-Bash's format and Perl regex syntax. A small translator turns them
into Rust regexes, and `tests/parity.rs` checks the output is byte-for-byte what Perl produces on
real Spotify files.

```
spotiflow.cmd        launcher for every platform
src/
  commands/          install, theme, uninstall, reinstall, update, status
  patch/             patch table and the Perl-to-Rust translator
    windows/         Spotify.dll patching for x64 and ARM64
  platform/          finding and handling Spotify on each OS
  bundle/            xpui.spa and the V8 snapshot
  upstream/          embedded and updated SpotX data
assets/              patch data, theme, logo
```

</details>

<details>
<summary><b>Development and testing</b></summary>

```sh
cargo test                      # unit tests
```

The parity test compares the Rust engine with Perl, patch by patch, on untouched files from any
platform. For example, against a Linux `.deb` unpacked on a Mac:

```sh
PARITY_OS=Linux PARITY_ARCH=x86_64 PARITY_VER=1.3.4.258 \
PARITY_SPA=usr/share/spotify/Apps/xpui.spa \
PARITY_SNAPSHOT=usr/share/spotify/v8_context_snapshot.bin \
PARITY_BIN=usr/share/spotify/spotify \
cargo test --release --test parity -- --ignored --nocapture
```

`PARITY_OS` is `macOS`, `Linux` or `Windows`. Leave out `PARITY_BIN` to check only the UI patches.

The Windows test checks the `Spotify.dll` patcher against hashes of SpotX's own output. Point it
at folders holding untouched `Spotify.dll`, `Spotify.exe` and `chrome_elf.dll`:

```sh
SPOTIFLOW_WIN_X64=path/x64 SPOTIFLOW_WIN_ARM64=path/arm64 \
cargo test --release --test windows -- --ignored
```

GitHub Actions run the rest:

| Workflow  | When                         | What                                                           |
| --------- | ---------------------------- | -------------------------------------------------------------- |
| `ci`      | Every push                   | Format, lint and tests on macOS, Linux and Windows             |
| `e2e`     | Release tags, weekly, manual | Patches a real Spotify on each OS and checks the UI loads      |
| `sync`    | Weekly, manual               | Pulls the newest SpotX-Bash and opens a pull request           |
| `release` | Release tags                 | Builds the six binaries `spotiflow.cmd` downloads              |

</details>

## Credits

Built on the work of [SpotX](https://github.com/SpotX-Official/SpotX) and
[SpotX-Bash](https://github.com/SpotX-Official/SpotX-Bash) by amd64fox and contributors (MIT).
The patch data, free-tier CSS and download mirror all come from them. Clean clients are fetched
from [LoaderSpot](https://github.com/LoaderSpot/table).

## License

MIT, with the SpotX notice included. See [LICENSE](LICENSE).
