# hudcc-rs Distribution Design

## Overview

Distribution system for hudcc-rs: a curl-based install script backed by GitHub Releases, with auto-update built into the HUD binary. Public GitHub repo, private by intent (no marketing).

## Naming

- **GitHub repo**: `hudcc-rs`
- **Cargo package**: `hudcc_rs`
- **Binary name**: `hudcc_rs`
- **Install location**: `~/.claude/hud/hudcc_rs`

## Targets

Three release binaries:

| Target                        | Runner       | Binary name                          |
|-------------------------------|--------------|--------------------------------------|
| `x86_64-unknown-linux-gnu`    | ubuntu-latest| `hudcc-rs-x86_64-unknown-linux-gnu`  |
| `x86_64-apple-darwin`         | macos-13     | `hudcc-rs-x86_64-apple-darwin`       |
| `aarch64-apple-darwin`        | macos-14     | `hudcc-rs-aarch64-apple-darwin`      |

Linux covers native Linux and WSL. macOS covers Intel and Apple Silicon.

## CI/CD: GitHub Actions Release Workflow

**File**: `.github/workflows/release.yml`

**Trigger**: Push a git tag matching `v*` (e.g., `v0.2.0`).

**Jobs**:

1. **test** — `ubuntu-latest`, runs `cargo test` to gate the release.
2. **build** — `needs: test`, matrix strategy over the 3 targets. Each job:
   - Installs the appropriate Rust target
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
3. Map to binary name:
   - `Linux` + `x86_64` → `hudcc-rs-x86_64-unknown-linux-gnu`
   - `Darwin` + `x86_64` → `hudcc-rs-x86_64-apple-darwin`
   - `Darwin` + `arm64` → `hudcc-rs-aarch64-apple-darwin`
4. Query GitHub API for latest release tag: `GET /repos/yonben/hudcc-rs/releases/latest`
5. Download binary to `~/.claude/hud/hudcc_rs`
6. `chmod +x`
7. Print installed version

**Error cases**:
- Unsupported OS/arch: print error listing supported platforms
- `~/.claude/` doesn't exist: print error suggesting Claude Code isn't installed
- Download failure: print error with HTTP status

## Auto-Update

The HUD binary checks for updates on each run, using the same background-thread pattern as the existing version and API checks.

### Check logic

- A background thread queries the GitHub Releases API: `GET /repos/yonben/hudcc-rs/releases/latest`
- Response cached to `~/.claude/hud/.hud-update-cache.json` with a **24-hour TTL**
- Compares the latest tag against the compiled-in version from `env!("CARGO_PKG_VERSION")`

### Update behavior

Controlled by `HUD_NO_AUTO_UPDATE` environment variable:

**Auto-update enabled (default)**:
- If a newer version exists:
  1. Download the new binary to `~/.claude/hud/hudcc_rs.tmp`
  2. Rename `hudcc_rs.tmp` → `hudcc_rs` (atomic replace)
  3. Show column: `HUD: ✓ updated to v0.2.0`
  4. Next HUD invocation runs the new version

**Auto-update disabled (`HUD_NO_AUTO_UPDATE=1`)**:
- If a newer version exists: show column: `HUD: ⬆ v0.2.0` (passive indicator)

**Already on latest**: no HUD column shown (zero noise).

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

## Files to Create/Modify

| File | Action |
|------|--------|
| `.github/workflows/release.yml` | Create — CI/CD workflow |
| `install.sh` | Create — curl install script |
| `Cargo.toml` | Modify — rename package to `hudcc_rs` |
| `src/update.rs` | Create — update check and auto-update logic |
| `src/main.rs` | Modify — add update thread, render update column |
| `src/lib.rs` | Modify — add `pub mod update` |
| `src/render.rs` | Modify — add HUD update status column |
