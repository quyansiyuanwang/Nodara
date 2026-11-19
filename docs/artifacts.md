# Nodara Build Artifacts

> Chinese: [artifacts.zh.md](artifacts.zh.md)

`scripts/build-artifacts.ps1 -Configuration All` builds the packages for the
**host** platform. The script detects Windows, macOS or Linux, names the package
`Nodara-<version>-<platform>-<arch>`, and writes either a `.zip` (Windows) or a
`.tar.gz` (macOS/Linux):

```text
artifacts/
├── debug/
│   ├── Nodara-2.3.0-windows-x86_64-debug/
│   ├── Nodara-2.3.0-windows-x86_64-debug.zip
│   └── Nodara-2.3.0-windows-x86_64-debug.zip.sha256
└── release/
    ├── Nodara-2.3.0-windows-x86_64/
    ├── Nodara-2.3.0-windows-x86_64.zip
    └── Nodara-2.3.0-windows-x86_64.zip.sha256
```

| Host | Portable archive | Installers |
|---|---|---|
| Windows x64 | `Nodara-<version>-windows-x86_64.zip` | NSIS `.exe`, MSI `.msi` |
| Linux x64 | `Nodara-<version>-linux-x86_64.tar.gz` | DEB, AppImage |
| macOS Apple Silicon | `Nodara-<version>-macos-aarch64.tar.gz` | DMG |
| macOS Intel | `Nodara-<version>-macos-x86_64.tar.gz` | DMG |

A release is built on each native runner; the Release workflow combines the four
packages into one GitHub Release. `artifacts/` is intentionally ignored by Git.

## Package contents

Every package contains the CLI, standalone runtime, Agent, Studio executable, web
build, both plugins, examples, schemas, documentation and `build-info.json`.
Debug packages on Windows also contain a `symbols/` directory with PDB files.

`SHA256SUMS.txt` covers every staged file. The release installers bundle the
runtime and both official plugins beside the application.

## Script options

| Option | Effect |
|---|---|
| `-Configuration Debug` | Build and archive only the debug flavour |
| `-Configuration Release` | Build and archive only the release flavour |
| `-Configuration All` | Both flavours (default) |
| `-SkipTests` | Skip Core/Agent/Studio tests; use only after a tested build |
| `-SkipBuild` | Re-stage and archive existing build outputs |

`-Configuration Release` builds only the release flavour. The script runs the
Core, Agent and Studio test suites before a normal build.

## Integrity

Verify the archive first:

```powershell
$archive = ".\artifacts\release\Nodara-2.3.0-windows-x86_64.zip"  # or .tar.gz
$expected = (Get-Content "$archive.sha256").Split()[0]
$actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
$expected -eq $actual
```

macOS/Linux:

```bash
shasum -a 256 -c Nodara-2.3.0-linux-x86_64.tar.gz.sha256
# or: sha256sum -c Nodara-2.3.0-linux-x86_64.tar.gz.sha256
```

Then verify the staged files:

```powershell
Set-Location .\artifacts\release\Nodara-2.3.0-windows-x86_64
Get-Content .\SHA256SUMS.txt
```

`scripts/verify-package.ps1` performs all of this and adds the CLI/runtime smoke
test; the Release workflow runs it before uploading anything.

## Build metadata

`build-info.json` records the platform and architecture in addition to the
version, commit and toolchain versions:

```json
{
  "product": "Nodara",
  "version": "2.3.0",
  "configuration": "Release",
  "platform": "windows",
  "arch": "x86_64",
  "source_commit": "abc1234",
  "installers": ["Nodara-Studio-2.3.0-x64-setup.exe", "Nodara-Studio-2.3.0-x64.msi"],
  "entrypoints": ["nodara-cli.exe", "nodara-runtime.exe", "nodara-agent.exe", "nodara-studio.exe", "studio-web/index.html"]
}
```

`tests_run` only records that the packaging script ran the test commands; failed
tests stop the script before a package is written.

## Published releases

A `v*` tag runs `.github/workflows/release.yml` on Windows, Linux and macOS,
verifies every archive with `scripts/verify-package.ps1`, and publishes the four
portable packages, their `.sha256` files, the platform installers, per-platform
build metadata and an asset-level `SHA256SUMS.txt`. The process and its recovery
paths are documented in [releasing.md](releasing.md).

To fetch and verify a published package:

```powershell
gh release download v2.3.0 -D .\artifacts\release
.\scripts\verify-package.ps1 `
  -ZipPath .\artifacts\release\Nodara-2.3.0-windows-x86_64.zip `
  -Version 2.3.0
```

## Requirements and reproducibility

- Windows x64 with MSVC, macOS 12+ with Xcode command-line tools, or Linux x64
  with the Tauri WebKitGTK 4.1 development packages.
- Rust 1.88 or newer; Node.js 20 or newer for the Studio.
- WebView2 on Windows, WKWebView on macOS, WebKitGTK 4.1 on Linux.
- The first MSI build downloads the WiX 3 toolchain; Linux AppImage bundling
  downloads `linuxdeploy`; both need network access.

`Cargo.lock` and `package-lock.json` are committed. Do not hand-edit versions,
lockfiles or package contents for an acceptance build; the version comes from the
workspace and the commit is recorded in `build-info.json`.

## Trust boundary

The packages are not code-signed. Windows SmartScreen and macOS Gatekeeper may
warn on first launch, and macOS additionally requires Accessibility and Screen
Recording permission for input and capture. Linux packages are unsigned. There is
no auto-update feed yet; adding one means a new workflow step and secret, not a
change to the layout above.
