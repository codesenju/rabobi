# Packaging Rabobi for Linux (Milestone 5.8)

Rabobi ships as a Linux desktop bundle built with `tauri build`. The
configured targets are **`.deb`**, **AppImage**, and **`.rpm`**.

> Packaging must run **on Linux** — Tauri builds native bundles for the host
> platform and cannot cross-build a Linux AppImage/.deb from macOS.

## Prerequisites (Debian/Ubuntu)

```bash
# Tauri build toolchain
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev build-essential curl wget file \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  rpm                              # only needed for the .rpm target

# Node 20+ and Rust (rustup)
```

## Build

```bash
cd rescueclone-desktop
npm install
npm run app:build        # == tauri build
```

Artifacts land under `src-tauri/target/release/bundle/`:

| Target | Path |
| --- | --- |
| Debian package | `bundle/deb/Rabobi_0.1.0_amd64.deb` |
| AppImage | `bundle/appimage/Rabobi_0.1.0_amd64.AppImage` |
| RPM | `bundle/rpm/Rabobi-0.1.0-1.x86_64.rpm` |

(Exact names vary with version/arch.)

## Runtime dependencies

The `.deb`/`.rpm` declare the required external tools as package dependencies,
so a normal `apt install ./Rabobi_*.deb` pulls them in:

`util-linux` (lsblk/sfdisk), `gdisk` (sgdisk), `partclone`, `pigz`, `zstd`,
`bzip2`, `coreutils`.

The AppImage does **not** bundle these system tools — install them separately
(`sudo apt-get install partclone gdisk pigz zstd`). The app's preflight check
(Home screen) reports any that are missing.

## Running with privileges

Real disk operations require root. Launch the installed app with elevation,
e.g.:

```bash
pkexec rabobi           # polkit prompt
# or
sudo -E rabobi
```

Without root the app still starts but refuses to begin a real operation (it
reports "Not running as root" on the Home screen).

## The CLI

The headless `rabobi` CLI binary (used by the test harness) is built
alongside the app:

```bash
cd src-tauri && cargo build --release --bin rabobi
./target/release/rabobi list-drives
```

To include it in a package, copy it into the bundle's resources or install it
to `/usr/bin` via a post-install step (left out of the default bundle to keep
the GUI package self-contained).

## Versioning

Bump `version` in both `package.json` and `src-tauri/tauri.conf.json` (they
must match), then rebuild.
