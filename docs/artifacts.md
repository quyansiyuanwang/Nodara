# Nodara Build Artifacts

> Chinese: [artifacts.zh.md](artifacts.zh.md)

`scripts/build-artifacts.ps1 -Configuration All` creates:

```text
artifacts/
├── debug/
│   ├── Nodara-2.1.0-windows-x86_64-debug/
│   ├── Nodara-2.1.0-windows-x86_64-debug.zip
│   └── Nodara-2.1.0-windows-x86_64-debug.zip.sha256
└── release/
    ├── Nodara-2.1.0-windows-x86_64/
    ├── Nodara-2.1.0-windows-x86_64.zip
    └── Nodara-2.1.0-windows-x86_64.zip.sha256
```

Each runnable package contains the CLI, standalone runtime, Agent, Studio desktop
executable, web build, two plugins, examples, schemas, documentation, and
`build-info.json`. Debug also contains a `symbols\` directory. Release also
contains NSIS and MSI installers. `SHA256SUMS.txt` covers every staged file.

`nodara-studio.exe` automatically starts or reuses the sibling
`nodara-runtime.exe`, so the portable package needs no two-terminal setup. The
release installers also bundle the runtime and both official plugins beside the
application.

The script accepts `-Configuration Debug|Release|All`, `-SkipTests`, and
`-SkipBuild`. Core, Agent, and Studio tests run before a normal build. Artifact
creation is local and `artifacts/` is intentionally ignored by Git.
`-Configuration Release` builds only the release flavour.

## Published releases

A `v*` tag runs `.github/workflows/release.yml`, which builds this release
package on `windows-latest`, verifies it with `scripts/verify-package.ps1`, and
publishes the portable ZIP, its SHA-256, both installers, `build-info.json` and
an asset-level `SHA256SUMS.txt` on the GitHub Releases page. The process and its
recovery paths are documented in [releasing.md](releasing.md).

To fetch and verify a published package:

```powershell
gh release download v2.1.0 -D .\artifacts\release
.\scripts\verify-package.ps1 `
  -ZipPath .\artifacts\release\Nodara-2.1.0-windows-x86_64.zip `
  -Version 2.1.0
```

Verify a package with:

```powershell
$zip = ".\artifacts\release\Nodara-2.1.0-windows-x86_64.zip"
(Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
Get-Content "$zip.sha256"
```

Debug uses Cargo's `dev` profile with debug information and separate PDB files.
Release uses `opt-level=3`, LTO, one codegen unit, and stripped binaries. Use
debug for reproduction and symbols; use release for acceptance and distribution.

The package is unsigned, so SmartScreen may warn. Only Windows x64 is currently
produced. MSI creation requires the WiX 3 toolchain, which Tauri downloads on
first use when the network is available.