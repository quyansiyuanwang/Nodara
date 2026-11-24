[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release", "All")]
    [string]$Configuration = "All",

    [switch]$SkipTests,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepoRoot = [System.IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$CoreRoot = Join-Path $RepoRoot "Nodara-Core"
$AgentRoot = Join-Path $RepoRoot "Nodara-Agent"
$StudioRoot = Join-Path $RepoRoot "Nodara-Studio"
$ArtifactsRoot = [System.IO.Path]::GetFullPath((Join-Path $RepoRoot "artifacts"))
$TestsRan = $false

$IsWindowsHost = $IsWindows
$IsMacHost = $IsMacOS
$Platform = if ($IsWindowsHost) { "windows" } elseif ($IsMacHost) { "macos" } else { "linux" }
$Architecture = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    "X64" { "x86_64" }
    "Arm64" { "aarch64" }
    "X86" { "i686" }
    default { "armv7" }
}
$ExecutableSuffix = if ($IsWindowsHost) { ".exe" } else { "" }
$Tauri = if ($IsWindowsHost) { ".\node_modules\.bin\tauri.cmd" } else { "./node_modules/.bin/tauri" }
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

function Invoke-InDirectory {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string]$Command,
        [Parameter(Mandatory)] [string[]]$Arguments
    )

    Push-Location $Path
    try {
        Write-Host "==> $Command $($Arguments -join ' ')  [$Path]" -ForegroundColor Cyan
        & $Command @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$Command exited with code $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

function Get-ProjectVersion {
    $cargoToml = Get-Content -Raw (Join-Path $CoreRoot "Cargo.toml")
    $match = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $match.Success) {
        throw "could not read the workspace version from Nodara-Core/Cargo.toml"
    }
    return $match.Groups[1].Value
}

function Copy-Required {
    param(
        [Parameter(Mandatory)] [string]$Source,
        [Parameter(Mandatory)] [string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "required build output is missing: $Source"
    }
    $parent = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

function New-StageDirectory {
    param([Parameter(Mandatory)] [string]$Path)

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $prefix = $ArtifactsRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing to remove a staging directory outside artifacts/: $fullPath"
    }
    if (Test-Path -LiteralPath $fullPath) {
        Remove-Item -LiteralPath $fullPath -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $fullPath | Out-Null
    return $fullPath
}

function Write-Checksums {
    param([Parameter(Mandatory)] [string]$Root)

    $lines = Get-ChildItem -LiteralPath $Root -Recurse -File |
        Where-Object { $_.Name -ne "SHA256SUMS.txt" } |
        Sort-Object FullName |
        ForEach-Object {
            $relative = [System.IO.Path]::GetRelativePath($Root, $_.FullName).Replace('\', '/')
            $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            "$hash  $relative"
        }
    [System.IO.File]::WriteAllText(
        (Join-Path $Root "SHA256SUMS.txt"),
        (($lines -join "`n") + "`n"),
        $Utf8NoBom
    )
}

function Select-VersionedBundleFile {
    param(
        [Parameter(Mandatory)] [string]$Directory,
        [Parameter(Mandatory)] [string]$Filter,
        [Parameter(Mandatory)] [string]$Version
    )

    if (-not (Test-Path -LiteralPath $Directory)) {
        return $null
    }
    $candidates = @(Get-ChildItem -LiteralPath $Directory -Filter $Filter -File -ErrorAction SilentlyContinue)
    $matching = @($candidates | Where-Object { $_.Name -like "*_${Version}_*" })
    if ($matching.Count -gt 0) {
        return $matching[0]
    }
    if ($candidates.Count -gt 0) {
        # A cached target directory can keep installers from an older release
        # beside the new ones. Copying the first match silently shipped the old
        # version once; refuse instead.
        throw ("no installer for version $Version in ${Directory}; found: " +
            ($candidates.Name -join ", "))
    }
    return $null
}

function Copy-Installers {
    param(
        [Parameter(Mandatory)] [ValidateSet("Debug", "Release")] [string]$Mode,
        [Parameter(Mandatory)] [string]$Stage,
        [Parameter(Mandatory)] [string]$Version
    )

    if ($Mode -eq "Debug") {
        return @()
    }

    $bundle = Join-Path $StudioRoot "src-tauri/target/$($Mode.ToLowerInvariant())/bundle"
    $destination = Join-Path $Stage "installers"
    New-Item -ItemType Directory -Force -Path $destination | Out-Null

    function Copy-InstallerFile {
        param([string]$Source, [string]$Name)
        if ($Source.EndsWith(".exe")) {
            $reported = (Get-Item -LiteralPath $Source).VersionInfo.ProductVersion
            if ($reported -and $reported -ne $Version) {
                throw "installer $Source reports version $reported, expected $Version"
            }
        }
        Copy-Item -LiteralPath $Source -Destination (Join-Path $destination $Name) -Force
        $script:copiedInstallers += $Name
    }

    if ($IsWindowsHost) {
        $nsis = Select-VersionedBundleFile -Directory (Join-Path $bundle "nsis") -Filter "*setup.exe" -Version $Version
        if ($nsis) {
            Copy-InstallerFile $nsis.FullName "Nodara-Studio-$Version-x64-setup.exe"
        }
        $msi = Select-VersionedBundleFile -Directory (Join-Path $bundle "msi") -Filter "*.msi" -Version $Version
        if ($msi) {
            Copy-InstallerFile $msi.FullName "Nodara-Studio-$Version-x64.msi"
        }
        $missing = @("Nodara-Studio-$Version-x64-setup.exe", "Nodara-Studio-$Version-x64.msi") |
            Where-Object { -not (Test-Path -LiteralPath (Join-Path $destination $_)) }
        if ($missing) {
            throw ("the Windows bundle did not produce: " + ($missing -join ", "))
        }
    }
    elseif ($IsMacHost) {
        $dmg = Select-VersionedBundleFile -Directory (Join-Path $bundle "dmg") -Filter "*.dmg" -Version $Version
        if ($dmg) {
            Copy-InstallerFile $dmg.FullName "Nodara-Studio-$Version-$Architecture.dmg"
        }
        else {
            Write-Warning "the macOS bundle did not produce a $Version DMG; the portable archive is still complete"
        }
    }
    else {
        $deb = Select-VersionedBundleFile -Directory (Join-Path $bundle "deb") -Filter "*.deb" -Version $Version
        if ($deb) {
            Copy-InstallerFile $deb.FullName "Nodara-Studio-$Version-$Architecture.deb"
        }
        $appimage = Select-VersionedBundleFile -Directory (Join-Path $bundle "appimage") -Filter "*.AppImage" -Version $Version
        if ($appimage) {
            Copy-InstallerFile $appimage.FullName "Nodara-Studio-$Version-$Architecture.AppImage"
        }
        if (-not $deb -and -not $appimage) {
            Write-Warning "the Linux bundle produced neither a $Version DEB nor an AppImage; the portable archive is still complete"
        }
    }

    return $script:copiedInstallers
}

function New-Package {
    param(
        [Parameter(Mandatory)] [ValidateSet("Debug", "Release")] [string]$Mode,
        [Parameter(Mandatory)] [string]$Version
    )

    $lower = $Mode.ToLowerInvariant()
    $suffix = if ($Mode -eq "Debug") { "-debug" } else { "" }
    $packageName = "Nodara-$Version-$Platform-$Architecture$suffix"
    $modeRoot = Join-Path $ArtifactsRoot $lower
    $stage = New-StageDirectory (Join-Path $modeRoot $packageName)
    $coreTarget = Join-Path $CoreRoot "target/$lower"
    $agentTarget = Join-Path $AgentRoot "target/$lower"
    $studioTarget = Join-Path $StudioRoot "src-tauri/target/$lower"

    foreach ($name in @("nodara-cli", "nodara-runtime", "nodara-platform-plugin", "nodara-vision-plugin")) {
        Copy-Required (Join-Path $coreTarget "$name$ExecutableSuffix") (Join-Path $stage "$name$ExecutableSuffix")
    }
    Copy-Required (Join-Path $agentTarget "nodara-agent$ExecutableSuffix") (Join-Path $stage "nodara-agent$ExecutableSuffix")
    Copy-Required (Join-Path $studioTarget "nodara-studio$ExecutableSuffix") (Join-Path $stage "nodara-studio$ExecutableSuffix")

    if ($Mode -eq "Debug" -and $IsWindowsHost) {
        foreach ($name in @(
            "nodara_cli.pdb",
            "nodara_runtime.pdb",
            "nodara_platform_plugin.pdb",
            "nodara_vision_plugin.pdb"
        )) {
            Copy-Required (Join-Path $coreTarget $name) (Join-Path $stage "symbols/$name")
        }
        Copy-Required (Join-Path $agentTarget "nodara_agent.pdb") (Join-Path $stage "symbols/nodara_agent.pdb")
        Copy-Required (Join-Path $studioTarget "nodara_studio.pdb") (Join-Path $stage "symbols/nodara_studio.pdb")
    }

    $pluginDefinitions = @(
        @{ Id = "nodara-platform"; Binary = "nodara-platform-plugin" },
        @{ Id = "nodara-vision"; Binary = "nodara-vision-plugin" }
    )
    foreach ($plugin in $pluginDefinitions) {
        $destination = Join-Path $stage "plugins/$($plugin.Id)"
        New-Item -ItemType Directory -Force -Path $destination | Out-Null
        Copy-Item -LiteralPath (Join-Path $CoreRoot "plugins/$($plugin.Id)/manifest.json") -Destination $destination -Force
        Copy-Required (Join-Path $coreTarget "$($plugin.Binary)$ExecutableSuffix") (Join-Path $destination "$($plugin.Binary)$ExecutableSuffix")
    }

    Copy-Item -LiteralPath (Join-Path $StudioRoot "dist") -Destination (Join-Path $stage "studio-web") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $CoreRoot "schema") -Destination (Join-Path $stage "schema") -Recurse -Force

    $examplesDestination = Join-Path $stage "examples"
    New-Item -ItemType Directory -Force -Path $examplesDestination | Out-Null
    Get-ChildItem -LiteralPath (Join-Path $RepoRoot "examples") -Filter "*.json" -File |
        Copy-Item -Destination $examplesDestination -Force

    foreach ($doc in @("README.md", "README.zh.md", "QUICKSTART.md", "QUICKSTART.zh.md")) {
        if (Test-Path -LiteralPath (Join-Path $RepoRoot $doc)) {
            Copy-Item -LiteralPath (Join-Path $RepoRoot $doc) -Destination $stage -Force
        }
    }

    # The destination must exist first: without it Copy-Item writes a *file*
    # named `docs`, which is how 2.0.0 shipped no documentation directory.
    $docsDestination = Join-Path $stage "docs"
    New-Item -ItemType Directory -Force -Path $docsDestination | Out-Null
    foreach ($doc in @(
        "user-manual.zh.md",
        "testing.zh.md",
        "artifacts.zh.md",
        "releasing.zh.md",
        "user-manual.md",
        "testing.md",
        "artifacts.md",
        "releasing.md"
    )) {
        $source = Join-Path $RepoRoot "docs/$doc"
        if (Test-Path -LiteralPath $source) {
            Copy-Item -LiteralPath $source -Destination $docsDestination -Force
        }
    }

    $referenceCore = Join-Path $stage "Nodara-Core"
    New-Item -ItemType Directory -Force -Path $referenceCore | Out-Null
    Copy-Item -LiteralPath (Join-Path $CoreRoot "docs") -Destination (Join-Path $referenceCore "docs") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $CoreRoot "protocol") -Destination (Join-Path $referenceCore "protocol") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $CoreRoot "plugins/README.md") -Destination $referenceCore -Force
    Copy-Item -LiteralPath (Join-Path $AgentRoot "README.md") -Destination (Join-Path $stage "Nodara-Agent-README.md") -Force
    Copy-Item -LiteralPath (Join-Path $StudioRoot "README.md") -Destination (Join-Path $stage "Nodara-Studio-README.md") -Force

    $script:copiedInstallers = @()
    $installers = @(Copy-Installers -Mode $Mode -Stage $stage -Version $Version)

    $commit = (& git -C $RepoRoot rev-parse --short HEAD).Trim()
    $rustVersion = (& rustc --version).Trim()
    $nodeVersion = (& node --version).Trim()
    $info = [ordered]@{
        product       = "Nodara"
        version       = $Version
        configuration = $Mode
        platform      = $Platform
        arch          = $Architecture
        target        = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
        built_at_utc  = [DateTime]::UtcNow.ToString("o")
        source_commit = $commit
        rust          = $rustVersion
        node          = $nodeVersion
        tests_run     = $TestsRan
        entrypoints   = @(
            "nodara-cli$ExecutableSuffix",
            "nodara-runtime$ExecutableSuffix",
            "nodara-agent$ExecutableSuffix",
            "nodara-studio$ExecutableSuffix",
            "studio-web/index.html"
        )
        installers    = $installers
    }
    $infoJson = $info | ConvertTo-Json -Depth 4
    [System.IO.File]::WriteAllText((Join-Path $stage "build-info.json"), $infoJson, $Utf8NoBom)
    Write-Checksums $stage

    $extension = if ($IsWindowsHost) { ".zip" } else { ".tar.gz" }
    $archivePath = Join-Path $modeRoot "$packageName$extension"
    if (Test-Path -LiteralPath $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }
    if ($IsWindowsHost) {
        Compress-Archive -LiteralPath $stage -DestinationPath $archivePath -CompressionLevel Optimal
    }
    else {
        & tar -czf $archivePath -C $modeRoot $packageName
        if ($LASTEXITCODE -ne 0) {
            throw "tar failed with exit code $LASTEXITCODE"
        }
    }

    $archiveHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText(
        (Join-Path $modeRoot "$packageName$extension.sha256"),
        "$archiveHash  $([System.IO.Path]::GetFileName($archivePath))`n",
        $Utf8NoBom
    )

    Write-Host "packaged: $archivePath" -ForegroundColor Green
}

if (-not $SkipBuild) {
    if (-not $SkipTests) {
        $TestsRan = $true
        Invoke-InDirectory $CoreRoot "cargo" @("test", "--workspace", "--no-fail-fast")
        Invoke-InDirectory $AgentRoot "cargo" @("test", "--workspace", "--no-fail-fast")
        Invoke-InDirectory $StudioRoot "npm" @("test")
    }

    # Build only the flavours that were asked for.
    if ($Configuration -in @("Debug", "All")) {
        Invoke-InDirectory $CoreRoot "cargo" @("build", "--workspace")
        Invoke-InDirectory $AgentRoot "cargo" @("build", "--workspace")
        Invoke-InDirectory $StudioRoot "npm" @("run", "build")
        Invoke-InDirectory $StudioRoot $Tauri @("build", "--debug", "--no-bundle", "--ci")
    }

    if ($Configuration -in @("Release", "All")) {
        Invoke-InDirectory $CoreRoot "cargo" @("build", "--workspace", "--release")
        Invoke-InDirectory $AgentRoot "cargo" @("build", "--workspace", "--release")
        Invoke-InDirectory $StudioRoot "npm" @("run", "build")
        # A cached target directory keeps the previous release's installers
        # beside the new ones. Remove the bundle output so this release is the
        # only thing Copy-Installers can see.
        $releaseBundle = Join-Path $StudioRoot "src-tauri/target/release/bundle"
        if (Test-Path -LiteralPath $releaseBundle) {
            Remove-Item -LiteralPath $releaseBundle -Recurse -Force
        }
        Invoke-InDirectory $StudioRoot $Tauri @("build", "--ci")
    }
}

$Version = Get-ProjectVersion
$modes = if ($Configuration -eq "All") { @("Debug", "Release") } else { @($Configuration) }
foreach ($mode in $modes) {
    New-Package -Mode $mode -Version $Version
}

Write-Host "artifacts are ready under: $ArtifactsRoot" -ForegroundColor Green
