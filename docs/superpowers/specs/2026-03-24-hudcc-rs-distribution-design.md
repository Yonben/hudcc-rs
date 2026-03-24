# hudcc-rs Distribution Design

## Overview

Distribution system for hudcc-rs: a curl-based install script backed by GitHub Releases, with auto-update built into the HUD binary. Public GitHub repo, private by intent (no marketing).

## Naming

- **GitHub repo**: `hudcc-rs`
- **Cargo package**: `hudcc_rs`
- **Binary name**: `hudcc_rs`
- **Install location**: `~/.claude/hud/hudcc_rs`

## Targets

Three release binaries, using `musl` for Linux (fully static, no glibc dependency):

| Target                        | Runner       | Binary name                          |
|-------------------------------|--------------|--------------------------------------|
| `x86_64-unknown-linux-musl`   | ubuntu-latest| `hudcc-rs-x86_64-unknown-linux-musl` |
| `x86_64-apple-darwin`         | macos-13     | `hudcc-rs-x86_64-apple-darwin`       |
| `aarch64-apple-darwin`        | macos-14     | `hudcc-rs-aarch64-apple-darwin`      |

Linux covers native Linux and WSL. macOS covers Intel and Apple Silicon. ARM64 Linux is not supported.

## CI/CD: GitHub Actions Release Workflow

**File**: `.github/workflows/release.yml`

**Trigger**: Push a git tag matching `v*` (e.g., `v0.2.0`).

**Permissions**: `contents: write` (required to create releases and upload assets).

**Jobs**:

1. **test** — `ubuntu-latest`, runs `cargo test` to gate the release.
2. **build** — `needs: test`, matrix strategy over the 3 targets. Each job:
   - Installs the appropriate Rust target (e.g., `rustup target add x86_64-unknown-linux-musl`)
   - For musl: installs `musl-tools` via apt
   - Runs `cargo build --release --target <target>`
   - Renames the binary to `hudcc-rs-<target>`
   - Uploads as a build artifact
3. **release** — `needs: build`:
   - Creates a GitHub Release from the tag
   - Attaches all 3 binaries

## Install Script

**File**: `install.sh` in the repo root.

**Usage**:
```bash
curl -fsSL https://raw.githubusercontent.com/yonben/hudcc-rs/main/install.sh | sh
```

**Logic**:
1. Detect OS via `uname -s` (`Linux`, `Darwin`)
2. Detect architecture via `uname -m` (`x86_64`, `arm64`/`aarch64`)
3. Map to asset name:
   - `Linux` + `x86_64` → `hudcc-rs-x86_64-unknown-linux-musl`
   - `Darwin` + `x86_64` → `hudcc-rs-x86_64-apple-darwin`
   - `Darwin` + `arm64` → `hudcc-rs-aarch64-apple-darwin`
4. Query GitHub API for latest release tag: `GET /repos/yonben/hudcc-rs/releases/latest`
5. Download the asset to `~/.claude/hud/hudcc_rs` (renaming from `hudcc-rs-<target>` to `hudcc_rs`)
6. `chmod +x`
7. Print installed version

**Error cases**:
- Unsupported OS/arch: print error listing supported platforms (Linux x86_64, macOS x86_64, macOS ARM64)
- `~/.claude/` doesn't exist: print error suggesting Claude Code isn't installed
- Download failure: print error with HTTP status

## Auto-Update

The HUD binary checks for updates on each run, using the same background-thread pattern as the existing version and API checks.

### Check logic

- A background thread queries the GitHub Releases API: `GET /repos/yonben/hudcc-rs/releases/latest`
- Response cached to `~/.claude/hud/.hud-update-cache.json` with a **24-hour TTL** (new constant `UPDATE_CACHE_TTL_MS = 86_400_000` in `cache.rs`)
- Compares the latest tag against the compiled-in version from `env!("CARGO_PKG_VERSION")`
- **Version comparison**: Parse version strings as `major.minor.patch` integers and compare numerically (not lexicographic string comparison)

### Platform detection

The auto-updater determines its target triple at **compile time** using `cfg` attributes:
- `#[cfg(all(target_os = "linux", target_arch = "x86_64"))]` → `hudcc-rs-x86_64-unknown-linux-musl`
- `#[cfg(all(target_os = "macos", target_arch = "x86_64"))]` → `hudcc-rs-x86_64-apple-darwin`
- `#[cfg(all(target_os = "macos", target_arch = "aarch64"))]` → `hudcc-rs-aarch64-apple-darwin`

### Update behavior

Controlled by `HUD_NO_AUTO_UPDATE` environment variable:

**Auto-update enabled (default)**:
- If a newer version exists:
  1. Download the new binary to `~/.claude/hud/hudcc_rs.tmp`
  2. Rename `hudcc_rs.tmp` → `hudcc_rs` (atomic on same filesystem; safe to replace running binary on both Linux and macOS since the OS holds the inode open)
  3. Show column: `HUD: ✓ updated to v0.2.0`
  4. Next HUD invocation runs the new version

**Auto-update disabled (`HUD_NO_AUTO_UPDATE=1`)**:
- If a newer version exists: show column: `HUD: ⬆ v0.2.0` (passive indicator)

**Already on latest**: no HUD column shown (zero noise).

### Error handling

GitHub API failures (403 rate limit, network errors, non-200 status) are handled gracefully: cache the failure and don't show any update column. Same pattern as the existing API error handling in `api.rs`.

### Self-location

The binary determines its own path via `std::env::current_exe()` so the update works regardless of where the binary is installed.

### Download mechanism

Uses the same `curl` subprocess approach as the existing API client in `src/api.rs`. Downloads from the GitHub Release asset URL matching the current platform.

## Versioning & Release Flow

Semantic versioning via `Cargo.toml`. Version baked into binary at compile time via `env!("CARGO_PKG_VERSION")`.

**To release**:
1. Bump `version` in `Cargo.toml`
2. Commit: `release: vX.Y.Z`
3. Tag: `git tag vX.Y.Z`
4. Push: `git push && git push --tags`
5. GitHub Actions builds and publishes automatically

## Cargo.toml Changes

- Rename `name` from `hud_rs` to `hudcc_rs`
- Keep `version` as the source of truth for the compiled-in version

## Render Column

The `HUD` update status is a **new column**, separate from the existing `Version` column (which shows Claude Code's version from npm). It only appears when there is something to report (update available or just updated).

## Files to Create/Modify

| File | Action |
|------|--------|
| `.github/workflows/release.yml` | Create — CI/CD workflow with `permissions: contents: write` |
| `install.sh` | Create — curl install script |
| `Cargo.toml` | Modify — rename package to `hudcc_rs` |
| `src/update.rs` | Create — update check, version comparison, auto-update logic with platform detection |
| `src/cache.rs` | Modify — add `UPDATE_CACHE_TTL_MS` constant |
| `src/main.rs` | Modify — update crate import from `hud_rs` to `hudcc_rs`, add update thread, pass result to render |
| `src/lib.rs` | Modify — add `pub mod update` |
| `src/render.rs` | Modify — add HUD update status column (separate from existing Version column) |
