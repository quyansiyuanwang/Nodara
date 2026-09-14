# Nodara Build Artifacts

> Chinese: [artifacts.zh.md](artifacts.zh.md)

`scripts/build-artifacts.ps1 -Configuration All` creates:

```text
artifacts/
├── debug/
│   ├── Nodara-2.0.0-windows-x86_64-debug/
│   ├── Nodara-2.0.0-windows-x86_64-debug.zip
│   └── Nodara-2.0.0-windows-x86_64-debug.zip.sha256
└── release/
    ├── Nodara-2.0.0-windows-x86_64/
    ├── Nodara-2.0.0-windows-x86_64.zip
    └── Nodara-2.0.0-windows-x86_64.zip.sha256
```

Each runnable package contains the CLI, standalone runtime, Agent, Studio desktop
executable, web build, two plugins, examples, schemas, documentation, and
`build-info.json`. Debug also contains a `symbols\` directory. Release also
contains NSIS and MSI installers. `SHA256SUMS.txt` covers every staged file.

The script accepts `-Configuration Debug|Release|All`, `-SkipTests`, and
`-SkipBuild`. Core, Agent, and Studio tests run before a normal build. Artifact
creation is local and `artifacts/` is intentionally ignored by Git.

Verify a package with:

```powershell
$zip = ".\artifacts\release\Nodara-2.0.0-windows-x86_64.zip"
(Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
Get-Content "$zip.sha256"
```

Debug uses Cargo's `dev` profile with debug information and separate PDB files.
Release uses `opt-level=3`, LTO, one codegen unit, and stripped binaries. Use
debug for reproduction and symbols; use release for acceptance and distribution.

The package is unsigned, so SmartScreen may warn. Only Windows x64 is currently
produced. MSI creation requires the WiX 3 toolchain, which Tauri downloads on
first use when the network is available.