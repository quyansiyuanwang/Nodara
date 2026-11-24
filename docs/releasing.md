# Releasing Nodara

> Chinese: [releasing.zh.md](releasing.zh.md)

This guide describes how a Nodara release is versioned, built, verified and
published. "One-command release" is the fast path; the rest explains what the
automation does so a failed run can be diagnosed without guessing.

The commands below use `<version>` as a placeholder for the release being cut
(for example `2.1.0`).

## Version model

The product version and the three contracts are independent:

| Version | Current source | Meaning |
|---|---|---|
| Product `<version>` | `Nodara-Core/Cargo.toml` `[workspace.package]` | The release, the installers and every executable |
| Workflow `schema_version` `2.1` | `nodara-schema` | The document format |
| Plugin `protocol_version` `1` | `nodara-schema` | JSON-RPC over stdio |
| API `api_version` `v1` | `nodara-runtime` | HTTP and WebSocket routes |

A product release never changes a contract version implicitly, and a contract
change does not force a product release by itself.

## One-command release

```powershell
# from the repository root, on a clean master that matches origin/master
.\scripts\release.ps1 -Version <version> -Title "schema-driven content hints"
```

The script:

1. Refuses to run on a dirty tree, off `master`, out of sync with
   `origin/master`, when the tag exists, or when a GitHub Release with that tag
   already exists.
2. Runs `scripts/set-version.ps1`, which rewrites every version location.
3. Finalizes `CHANGELOG.md`: the `## [Unreleased]` section becomes
   `## [<version>] — … - YYYY-MM-DD`, and a new empty `## [Unreleased]` is inserted
   above it.
4. Runs `scripts/check-version.ps1 -Version <version> -RequireChangelog`.
5. Commits `chore(release): v<version>`, creates the annotated tag `v<version>`, and
   pushes `master` and the tag in one push.
6. Prints the Release workflow URL.

Useful switches:

| Switch | Effect |
|---|---|
| `-NoPush` | Commit and tag locally, print the push command |
| `-TagOnly` | Tag the current `HEAD` without bumping or committing |
| `-TagOnly -Force` | Move an existing tag to `HEAD` (only when no GitHub Release exists) |
| `-Date 2026-11-17` | Override the CHANGELOG date |
| `-Title "…"` | Subtitle after the version in the CHANGELOG heading |

### Version locations

`set-version.ps1` and `check-version.ps1` cover the same list. Do not edit these
by hand:

- `Nodara-Core/Cargo.toml` — `[workspace.package].version`
- `Nodara-Core/Cargo.lock` — the eight workspace crates
- `Nodara-Agent/Cargo.toml` — `[workspace.package].version`
- `Nodara-Agent/Cargo.lock` — `nodara-agent`, `nodara-schema`
- `Nodara-Studio/package.json`
- `Nodara-Studio/package-lock.json` — the root and the `""` workspace entry
- `Nodara-Studio/src-tauri/Cargo.toml` — `[package].version`
- `Nodara-Studio/src-tauri/Cargo.lock` — `nodara-studio`
- `Nodara-Studio/src-tauri/tauri.conf.json`
- `Nodara-Core/plugins/nodara-platform/manifest.json`
- `Nodara-Core/plugins/nodara-vision/manifest.json`

The protocol examples and the test fixtures keep their literal `2.0.0` strings on
purpose; they are illustrations, not the product version.

## What CI does

`.github/workflows/release.yml` runs on any `v*` tag and has two jobs.

### `build` (matrix)

| Runner | Package | Installers |
|---|---|---|
| `windows-latest` | `Nodara-<version>-windows-x86_64.zip` | NSIS + MSI |
| `ubuntu-latest` | `Nodara-<version>-linux-x86_64.tar.gz` | DEB + AppImage |
| `macos-15` | `Nodara-<version>-macos-aarch64.tar.gz` | DMG |
| `macos-15-intel` | `Nodara-<version>-macos-x86_64.tar.gz` | DMG |

Every matrix job runs the same gates:

| Step | Gate |
|---|---|
| Version manifests | `check-version.ps1 -Version … -Tag … -RequireChangelog` |
| Formatting | `cargo fmt --all -- --check` in both Rust workspaces |
| Linting | `cargo clippy --workspace --all-targets -- -D warnings` in both |
| Schemas | Regenerates `Nodara-Core/schema` and fails on any diff |
| Tests | Core, Agent and Studio suites via `build-artifacts.ps1` |
| Build | Release binaries, Tauri bundle, portable archive |
| Package verification | `verify-package.ps1` checks hashes, entrypoints, CLI smoke and the runtime API |
| Upload | `release-assets-<platform>-<arch>`, kept for seven days |

`verify-package.ps1` expands the archive and checks that:

- the archive hash matches its `.sha256`, and every line of `SHA256SUMS.txt` matches;
- CLI, runtime, Agent, Studio, both plugins, `studio-web/`, `schema/`, `examples/`
  and `docs/` exist, and every installer listed in `build-info.json` is present and
  reports the release version (the setup's `ProductVersion` and the MSI's
  `ProductVersion` property);
- `build-info.json` names the release version, `Release`, the platform and the
  tagged commit;
- `nodara-cli --version`, `validate`, `simulate`, `run`, `inspect` and
  `extensions --json` succeed;
- `nodara-runtime` answers `/api/v1/health` with 2 plugins and 19 node types,
  `/api/v1/plugins` has no failures, `/api/v1/extensions` is non-empty, and
  `/api/v1/schema/workflow` returns a schema.

### `publish` (windows-latest, tags only)

Downloads every matrix artifact, generates one asset-level `SHA256SUMS.txt` and
the release notes from the CHANGELOG section, creates the GitHub Release with
`--verify-tag --latest` (or `--prerelease` for a version with a `-suffix`), then
re-reads the release to assert the per-platform asset list and flags.

Release assets:

| Asset | Contents |
|---|---|
| `Nodara-<version>-windows-x86_64.zip` | Windows portable package |
| `Nodara-<version>-linux-x86_64.tar.gz` | Linux portable package |
| `Nodara-<version>-macos-aarch64.tar.gz` | macOS Apple Silicon package |
| `Nodara-<version>-macos-x86_64.tar.gz` | macOS Intel package |
| `Nodara-Studio-<version>-x64-setup.exe` / `.msi` | Windows installers |
| `Nodara-Studio-<version>-<arch>.deb` / `.AppImage` | Linux installers |
| `Nodara-Studio-<version>-<arch>.dmg` | macOS disk image |
| `Nodara-<version>-<platform>-<arch>-build-info.json` | Build metadata |
| `SHA256SUMS.txt` | Checksums of every asset |

## Manual and dry-run builds

A manual run of the Release workflow (`workflow_dispatch`, input `version`) runs
the same gates and uploads `release-assets-*`, but never creates a release. Use it
to test a pipeline change before tagging.

The same packaging can be produced locally; `build-artifacts.ps1` detects the host
platform and writes a ZIP on Windows or a `tar.gz` on macOS and Linux:

```powershell
.\scripts\build-artifacts.ps1 -Configuration Release
.\scripts\verify-package.ps1 `
  -ZipPath .\artifacts\release\Nodara-<version>-<platform>-<arch>.zip `
  -Version <version>
```

`-Configuration Release` builds only the release flavour. `-SkipTests` and
`-SkipBuild` are available for iteration, but a release is never cut from a
package that skipped its tests.

## Verify a published release

```powershell
gh release download v<version> -D .\artifacts\tmp\release-verify
.\scripts\verify-package.ps1 `
  -ZipPath .\artifacts\tmp\release-verify\Nodara-<version>-windows-x86_64.zip `
  -Version <version>
```

The release notes print the verification commands for PowerShell, `shasum` and
`sha256sum`; every archive has a matching `.sha256` file.

## Recovery

| Situation | Recovery |
|---|---|
| Build job failed, no release created | Cancel the run (`gh run cancel <run-id>`), fix on `master`, then `.\scripts\release.ps1 -Version <version> -TagOnly -Force` |
| Publish job failed after a successful build | `gh run rerun <run-id> --failed`; the workflow artifact is kept for seven days |
| A release was created but is wrong | `gh release delete v<version> --yes`, fix, then `-TagOnly -Force` |
| Tag exists but the commit is wrong | `git push origin :refs/tags/v<version>`, fix, then `-TagOnly -Force` |
| Version manifests drifted | `.\scripts\check-version.ps1 -Version <version> -Tag v<version> -RequireChangelog` |
| CHANGELOG section missing | Add the `## [<version>]` section (or rerun `release.ps1`, which writes it) |

`-TagOnly -Force` refuses to run when the GitHub Release already exists, so it
cannot silently republish over a released version.

## Prereleases and hotfixes

- `v2.2.0-rc.1` is a valid tag: the workflow marks it `--prerelease` and does not
  move the `latest` pointer.
- A hotfix is a normal patch release from `master` (`2.1.1`), never a rewritten
  tag. Users verify downloads by checksum, so replacing an asset under the same
  version is not supported.

## Security notes

- The packages are **not code-signed**. Windows SmartScreen and macOS Gatekeeper
  may warn on first launch; the release notes tell users to verify the checksum
  first and how to open the app. Linux packages are unsigned too.
- The publish job uses the repository `GITHUB_TOKEN` with `contents: write`; no
  long-lived personal token is stored in the repository.
- No signing key, notarisation or auto-update feed exists yet. Adding one means
  a new workflow step and a new secret, not a change to this process.
