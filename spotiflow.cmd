:; set -eu; repo=lunikos/spotiflow; here=$(cd "$(dirname "$0")" && pwd); cargo=$(command -v cargo || echo "$HOME/.cargo/bin/cargo")
:; if [ -f "$here/Cargo.toml" ] && [ -x "$cargo" ]; then
:;   bin="$here/target/release/spotiflow"
:;   if [ ! -x "$bin" ] || [ -n "$(find "$here/src" "$here/assets" "$here/Cargo.toml" -newer "$bin" | head -1)" ]; then "$cargo" build --release --quiet --manifest-path "$here/Cargo.toml"; fi
:; else
:;   os=$(uname -s | sed 's/Darwin/macos/;s/Linux/linux/'); arch=$(uname -m | sed 's/aarch64/arm64/;s/amd64/x86_64/')
:;   dir=${XDG_CACHE_HOME:-$HOME/.cache}/spotiflow; bin="$dir/spotiflow"; mkdir -p "$dir"
:;   if [ ! -x "$bin" ]; then
:;     command -v gh >/dev/null || { echo "Install the GitHub CLI (gh) and run 'gh auth login', or install Rust: https://rustup.rs" >&2; exit 1; }
:;     gh release download -R "$repo" -p "spotiflow-$os-$arch" -O "$bin" --clobber && chmod +x "$bin"
:;   fi
:; fi
:; exec "$bin" "$@"
@echo off
setlocal
set "repo=lunikos/spotiflow"
set "here=%~dp0"
if exist "%here%Cargo.toml" (
  where cargo >nul 2>nul && (
    cargo build --release --quiet --manifest-path "%here%Cargo.toml" || exit /b 1
    "%here%target\release\spotiflow.exe" %*
    exit /b
  )
)
set "arch=x86_64"
if /i "%PROCESSOR_ARCHITECTURE%"=="ARM64" set "arch=arm64"
set "bin=%LOCALAPPDATA%\spotiflow\spotiflow.exe"
if not exist "%bin%" (
  where gh >nul 2>nul || (echo Install the GitHub CLI ^(gh^) and run "gh auth login", or install Rust: https://rustup.rs & exit /b 1)
  mkdir "%LOCALAPPDATA%\spotiflow" 2>nul
  gh release download -R %repo% -p "spotiflow-windows-%arch%.exe" -O "%bin%" --clobber || exit /b 1
)
"%bin%" %*
