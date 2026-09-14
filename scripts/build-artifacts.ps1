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
    [System.IO.File]::WriteAllLines(
        (Join-Path $Root "SHA256SUMS.txt"),
        $lines,
        [System.Text.UTF8Encoding]::new($false)
    )
}

function New-Package {
    param(
        [Parameter(Mandatory)] [ValidateSet("Debug", "Release")] [string]$Mode,
        [Parameter(Mandatory)] [string]$Version
    )

    $lower = $Mode.ToLowerInvariant()
    $suffix = if ($Mode -eq "Debug") { "-debug" } else { "" }
    $packageName = "Nodara-$Version-windows-x86_64$suffix"
    $modeRoot = Join-Path $ArtifactsRoot $lower
    $stage = New-StageDirectory (Join-Path $modeRoot $packageName)
    $coreTarget = Join-Path $CoreRoot "target\$lower"
    $agentTarget = Join-Path $AgentRoot "target\$lower"
    $studioTarget = Join-Path $StudioRoot "src-tauri\target\$lower"

    foreach ($name in @("nodara-cli.exe", "nodara-runtime.exe", "nodara-platform-plugin.exe", "nodara-vision-plugin.exe")) {
        Copy-Required (Join-Path $coreTarget $name) (Join-Path $stage $name)
    }
    Copy-Required (Join-Path $agentTarget "nodara-agent.exe") (Join-Path $stage "nodara-agent.exe")
    Copy-Required (Join-Path $studioTarget "nodara-studio.exe") (Join-Path $stage "nodara-studio.exe")

    if ($Mode -eq "Debug") {
        foreach ($name in @(
            "nodara_cli.pdb",
            "nodara_runtime.pdb",
            "nodara_platform_plugin.pdb",
            "nodara_vision_plugin.pdb"
        )) {
            Copy-Required (Join-Path $coreTarget $name) (Join-Path $stage "symbols\$name")
        }
        Copy-Required (Join-Path $agentTarget "nodara_agent.pdb") (Join-Path $stage "symbols\nodara_agent.pdb")
        Copy-Required (Join-Path $studioTarget "nodara_studio.pdb") (Join-Path $stage "symbols\nodara_studio.pdb")
    }

    $pluginDefinitions = @(
        @{ Id = "nodara-platform"; Binary = "nodara-platform-plugin.exe" },
        @{ Id = "nodara-vision"; Binary = "nodara-vision-plugin.exe" }
    )
    foreach ($plugin in $pluginDefinitions) {
        $destination = Join-Path $stage "plugins\$($plugin.Id)"
        New-Item -ItemType Directory -Force -Path $destination | Out-Null
        Copy-Item -LiteralPath (Join-Path $CoreRoot "plugins\$($plugin.Id)\manifest.json") -Destination $destination -Force
        Copy-Required (Join-Path $coreTarget $plugin.Binary) (Join-Path $destination $plugin.Binary)
    }

    Copy-Item -LiteralPath (Join-Path $StudioRoot "dist") -Destination (Join-Path $stage "studio-web") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $CoreRoot "schema") -Destination (Join-Path $stage "schema") -Recurse -Force

    $examplesDestination = Join-Path $stage "examples"
    New-Item -ItemType Directory -Force -Path $examplesDestination | Out-Null
    Get-ChildItem -LiteralPath (Join-Path $RepoRoot "examples") -Filter "*.json" -File |
        Copy-Item -Destination $examplesDestination -Force

    foreach ($doc in @("README.zh.md", "QUICKSTART.zh.md")) {
        if (Test-Path -LiteralPath (Join-Path $RepoRoot $doc)) {
            Copy-Item -LiteralPath (Join-Path $RepoRoot $doc) -Destination $stage -Force
        }
    }
    foreach ($doc in @("user-manual.zh.md", "testing.zh.md", "artifacts.zh.md", "user-manual.md", "testing.md", "artifacts.md")) {
        $source = Join-Path $RepoRoot "docs\$doc"
        if (Test-Path -LiteralPath $source) {
            Copy-Item -LiteralPath $source -Destination (Join-Path $stage "docs") -Force
        }
    }

    $referenceCore = Join-Path $stage "Nodara-Core"
    New-Item -ItemType Directory -Force -Path $referenceCore | Out-Null
    Copy-Item -LiteralPath (Join-Path $CoreRoot "docs") -Destination (Join-Path $referenceCore "docs") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $CoreRoot "protocol") -Destination (Join-Path $referenceCore "protocol") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $CoreRoot "plugins\README.md") -Destination $referenceCore -Force
    Copy-Item -LiteralPath (Join-Path $AgentRoot "README.md") -Destination (Join-Path $stage "Nodara-Agent-README.md") -Force
    Copy-Item -LiteralPath (Join-Path $StudioRoot "README.md") -Destination (Join-Path $stage "Nodara-Studio-README.md") -Force

    if ($Mode -eq "Release") {
        $installerDestination = Join-Path $stage "installers"
        New-Item -ItemType Directory -Force -Path $installerDestination | Out-Null
        Copy-Required (Join-Path $studioTarget "bundle\nsis\Nodara Studio_$Version`_x64-setup.exe") (Join-Path $installerDestination "Nodara-Studio-$Version-x64-setup.exe")
        Copy-Required (Join-Path $studioTarget "bundle\msi\Nodara Studio_$Version`_x64_en-US.msi") (Join-Path $installerDestination "Nodara-Studio-$Version-x64.msi")
    }

    $commit = (& git -C $RepoRoot rev-parse --short HEAD).Trim()
    $rustVersion = (& rustc --version).Trim()
    $nodeVersion = (& node --version).Trim()
    $info = [ordered]@{
        product          = "Nodara"
        version          = $Version
        configuration    = $Mode
        target           = "x86_64-pc-windows-msvc"
        built_at_utc     = [DateTime]::UtcNow.ToString("o")
        source_commit    = $commit
        rust             = $rustVersion
        node             = $nodeVersion
        tests_run        = $TestsRan
        entrypoints      = @(
            "nodara-cli.exe",
            "nodara-runtime.exe",
            "nodara-agent.exe",
            "nodara-studio.exe",
            "studio-web\index.html"
        )
    }
    $infoJson = $info | ConvertTo-Json -Depth 4
    [System.IO.File]::WriteAllText((Join-Path $stage "build-info.json"), $infoJson, [System.Text.UTF8Encoding]::new($false))
    Write-Checksums $stage

    $zipPath = Join-Path $modeRoot "$packageName.zip"
    if (Test-Path -LiteralPath $zipPath) {
        Remove-Item -LiteralPath $zipPath -Force
    }
    Compress-Archive -LiteralPath $stage -DestinationPath $zipPath -CompressionLevel Optimal

    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText(
        (Join-Path $modeRoot "$packageName.zip.sha256"),
        "$zipHash  $([System.IO.Path]::GetFileName($zipPath))
",
        [System.Text.UTF8Encoding]::new($false)
    )

    Write-Host "packaged: $zipPath" -ForegroundColor Green
}

if (-not $SkipBuild) {
    if (-not $SkipTests) {
        $TestsRan = $true
        Invoke-InDirectory $CoreRoot "cargo" @("test", "--workspace", "--no-fail-fast")
        Invoke-InDirectory $AgentRoot "cargo" @("test", "--workspace", "--no-fail-fast")
        Invoke-InDirectory $StudioRoot "npm" @("test")
    }

    Invoke-InDirectory $CoreRoot "cargo" @("build", "--workspace")
    Invoke-InDirectory $AgentRoot "cargo" @("build", "--workspace")
    Invoke-InDirectory $StudioRoot "npm" @("run", "build")
    Invoke-InDirectory $StudioRoot ".\node_modules\.bin\tauri.cmd" @("build", "--debug", "--no-bundle", "--ci")

    if ($Configuration -in @("Release", "All")) {
        Invoke-InDirectory $CoreRoot "cargo" @("build", "--workspace", "--release")
        Invoke-InDirectory $AgentRoot "cargo" @("build", "--workspace", "--release")
        Invoke-InDirectory $StudioRoot ".\node_modules\.bin\tauri.cmd" @("build", "--bundles", "nsis", "msi", "--ci", "--config", "src-tauri/tauri.release.conf.json")
    }
}

$Version = Get-ProjectVersion
$modes = if ($Configuration -eq "All") { @("Debug", "Release") } else { @($Configuration) }
foreach ($mode in $modes) {
    New-Package -Mode $mode -Version $Version
}

Write-Host "artifacts are ready under: $ArtifactsRoot" -ForegroundColor Green