# Continuous integration & release builds

Rabobi ships two GitHub Actions workflows under `.github/workflows/`.

## `ci.yml` — checks on every push / PR

- **Frontend** (Ubuntu): `tsc --noEmit` + `npm run build`.
- **Rust** (Ubuntu, Windows, macOS matrix): `cargo test --lib --bins` and
  `cargo clippy --lib -- -D warnings`.
  Runs with `RABOBI_SKIP_TAURI_BUILD=1` so the test job doesn't need the full
  Tauri bundling toolchain (icons, WebView, etc.) — it only compiles and tests
  the engine code. This is what validates the Windows/macOS code paths on their
  real OSes without doing a full app build.

## `release.yml` — native installers

Builds the real bundles with [`tauri-apps/tauri-action`](https://github.com/tauri-apps/tauri-action)
across a platform matrix:

| Runner | Target | Produces |
| --- | --- | --- |
| `macos-latest` | `aarch64-apple-darwin` | `.dmg`, `.app` (Apple Silicon) |
| `macos-latest` | `x86_64-apple-darwin` | `.dmg`, `.app` (Intel) |
| `ubuntu-latest` | `x86_64-unknown-linux-gnu` | `.deb`, `.rpm`, `.AppImage` |
| `windows-latest` | `x86_64-pc-windows-msvc` | `.msi`, `.exe` (NSIS) |

**Triggers:**
- **Push a tag** `vX.Y.Z` → builds every target and attaches the installers to a
  **draft GitHub Release** named `Rabobi vX.Y.Z`. Review and publish it manually.
- **Manual "Run workflow"** (`workflow_dispatch`) → builds every target and
  uploads the bundles as **workflow artifacts** (no release). Good for testing a
  branch.

Every run also uploads a per-target `rabobi-<triple>` artifact (14-day retention)
regardless of trigger.

### Cutting a release

```bash
# bump version in BOTH package.json and src-tauri/tauri.conf.json first, then:
git tag v0.1.0
git push origin v0.1.0
```

Watch the **Actions** tab; when all four jobs finish, a draft release holds the
installers. Publish it when ready.

## Code signing (not configured)

The CI builds are **unsigned**. Consequences for end users:

- **macOS** — Gatekeeper will warn "unidentified developer"; users right-click →
  Open, or you notarize with an Apple Developer ID (`APPLE_CERTIFICATE`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` secrets + enabling
  `tauri.conf.json > bundle > macOS`).
- **Windows** — SmartScreen may warn until the `.exe`/`.msi` is signed with an
  EV/OV code-signing certificate.
- **Linux** — `.deb`/`.rpm`/AppImage don't require signing to run.

Signing needs paid certificates and org secrets, so it's intentionally left as an
opt-in follow-up. The unsigned artifacts are fully functional for internal use
and testing.

## Caveat: the binary/crate name

The Cargo package is still `rescueclone-desktop` (crate id) while the product is
**Rabobi** (`productName` in `tauri.conf.json`). `tauri-action` names the
installers from `productName`, so the artifacts come out as `Rabobi_*`.
