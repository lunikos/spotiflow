<p align="center">
  <img src="assets/logo.svg" width="96" alt="">
</p>

<h1 align="center">spotiflow</h1>

<p align="center">
  Spotify, without the ads and the clutter.<br>
  One small tool for macOS, Linux and Windows.
</p>

---

spotiflow patches the Spotify desktop app so it plays without ads, then hides a few things you
probably never click (What's New, Friend Activity, Related videos). Everything else looks and
works like normal Spotify.

It runs the same patches as [SpotX](https://github.com/SpotX-Official/SpotX), rewritten as one
fast Rust binary. A full install takes about 7 seconds, and the binary is 1.5 MB.

## Get started

You need one file: `spotiflow.cmd`. It works on every platform.

```sh
sh spotiflow.cmd install        # macOS, Linux
spotiflow.cmd install           # Windows
```

Start Spotify and you're done. After Spotify updates itself, run `install` again.

The first run fetches the right prebuilt binary through the [GitHub CLI](https://cli.github.com)
(run `gh auth login` once first). If you've cloned the repo and have [Rust](https://rustup.rs),
it builds from source instead.

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
| `--cache-mb <N>`  | Cap Spotify's disk cache at N MB              |
| `-P <path>`       | Point at a Spotify install it can't find      |

</details>

## Staying up to date

SpotX changes often. spotiflow keeps up in two ways:

- **`spotiflow update`** fetches the newest patch set from SpotX-Bash. It records the exact
  commit and only keeps the update if every pattern compiles.
- **A weekly GitHub Action** does the same for the repo and opens a pull request with the tests
  already run.

Patches inject code into Spotify. Updating trusts upstream SpotX exactly as much as running SpotX
would.

<details>
<summary><b>How it works</b></summary>

1. Find Spotify, close it, and back it up the first time (or restore that backup on later runs).
2. Unpack `xpui.spa`, Spotify's web UI. Newer builds hide part of their code in the V8 snapshot,
   so that's copied out first.
3. Apply the patches that match your version, OS and CPU. On Windows, also patch `Spotify.dll`
   and strip the signatures SpotX strips from `Spotify.exe` and `chrome_elf.dll`.
4. Add the theme stylesheet, repack, and re-sign the app on macOS.

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
