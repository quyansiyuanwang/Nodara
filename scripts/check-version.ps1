[CmdletBinding()]
param(
    # Expected product version. Defaults to the version declared by Nodara-Core.
    [string]$Version,

    # Release tag to verify against, e.g. `v2.1.0`. Optional.
    [string]$Tag,

    # Require a non-empty `## [<version>]` section in CHANGELOG.md.
    [switch]$RequireChangelog,

    [string]$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$CoreRoot = Join-Path $RepoRoot "Nodara-Core"
$AgentRoot = Join-Path $RepoRoot "Nodara-Agent"
$StudioRoot = Join-Path $RepoRoot "Nodara-Studio"
$ChangelogPath = Join-Path $RepoRoot "CHANGELOG.md"

function Read-Text {
    param([Parameter(Mandatory)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "missing file: $Path"
    }
    return [System.IO.File]::ReadAllText($Path)
}

function Get-CargoVersion {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Section
    )

    $text = Read-Text $Path
    $pattern = '(?ms)^\[' + [regex]::Escape($Section) + '\][^\[]*?^version\s*=\s*"([^"]+)"'
    $match = [regex]::Match($text, $pattern)
    if (-not $match.Success) {
        throw "could not read the [$Section] version from $Path"
    }
    return $match.Groups[1].Value
}

function Get-JsonVersion {
    param(
        [Parameter(Mandatory)][string]$Path,
        [string]$Property = "version"
    )

    $json = (Read-Text $Path) | ConvertFrom-Json
    $value = $json.$Property
    if ($null -eq $value) {
        throw "could not read '$Property' from $Path"
    }
    return [string]$value
}

function Get-LockfileVersion {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Package
    )

    $lines = [System.IO.File]::ReadAllLines($Path)
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match '^name = "(.+)"$' -and $Matches[1] -eq $Package) {
            $limit = [Math]::Min($i + 4, $lines.Count)
            for ($j = $i + 1; $j -lt $limit; $j++) {
                if ($lines[$j] -match '^version = "(.+)"$') {
                    return $Matches[1]
                }
            }
        }
    }
    return $null
}

$expected = $Version
if (-not $expected) {
    $expected = Get-CargoVersion (Join-Path $CoreRoot "Cargo.toml") "workspace.package"
}
if ($expected -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
    throw "invalid product version: $expected"
}

$checks = @()
function Add-Check {
    param(
        [Parameter(Mandatory)][string]$Name,
        [AllowNull()][string]$Actual
    )
    $script:checks += [pscustomobject]@{ Name = $Name; Actual = $Actual }
}

Add-Check "Nodara-Core/Cargo.toml" (Get-CargoVersion (Join-Path $CoreRoot "Cargo.toml") "workspace.package")
Add-Check "Nodara-Agent/Cargo.toml" (Get-CargoVersion (Join-Path $AgentRoot "Cargo.toml") "workspace.package")
Add-Check "Nodara-Studio/package.json" (Get-JsonVersion (Join-Path $StudioRoot "package.json"))

$lockPath = Join-Path $StudioRoot "package-lock.json"
# `-AsHashtable` is required because npm writes a `""` package entry.
$lock = (Read-Text $lockPath) | ConvertFrom-Json -AsHashtable
Add-Check "Nodara-Studio/package-lock.json" ([string]$lock["version"])
if ($lock.ContainsKey("packages") -and $lock["packages"].ContainsKey("")) {
    Add-Check "Nodara-Studio/package-lock.json packages['']" ([string]$lock["packages"][""]["version"])
}
else {
    Add-Check "Nodara-Studio/package-lock.json packages['']" $null
}

Add-Check "Nodara-Studio/src-tauri/Cargo.toml" (Get-CargoVersion (Join-Path $StudioRoot "src-tauri/Cargo.toml") "package")
Add-Check "Nodara-Studio/src-tauri/tauri.conf.json" (Get-JsonVersion (Join-Path $StudioRoot "src-tauri/tauri.conf.json"))

Add-Check "plugins/nodara-platform/manifest.json" (Get-JsonVersion (Join-Path $CoreRoot "plugins/nodara-platform/manifest.json"))
Add-Check "plugins/nodara-vision/manifest.json" (Get-JsonVersion (Join-Path $CoreRoot "plugins/nodara-vision/manifest.json"))

$corePackages = @(
    "nodara-schema", "nodara-core", "nodara-plugin", "nodara-runtime",
    "nodara-platform", "nodara-vision", "nodara-cli", "nodara-testkit"
)
foreach ($package in $corePackages) {
    Add-Check "Nodara-Core/Cargo.lock:$package" (Get-LockfileVersion (Join-Path $CoreRoot "Cargo.lock") $package)
}
foreach ($package in @("nodara-agent", "nodara-schema")) {
    Add-Check "Nodara-Agent/Cargo.lock:$package" (Get-LockfileVersion (Join-Path $AgentRoot "Cargo.lock") $package)
}
Add-Check "Nodara-Studio/src-tauri/Cargo.lock:nodara-studio" (Get-LockfileVersion (Join-Path $StudioRoot "src-tauri/Cargo.lock") "nodara-studio")

$failures = @()
foreach ($check in $checks) {
    if ($check.Actual -ne $expected) {
        $shown = if ($null -eq $check.Actual) { "<missing>" } else { $check.Actual }
        Write-Host ("FAIL  {0}: expected {1}, found {2}" -f $check.Name, $expected, $shown) -ForegroundColor Red
        $failures += $check.Name
    }
    else {
        Write-Host ("ok    {0}: {1}" -f $check.Name, $check.Actual)
    }
}

if ($Tag) {
    if ($Tag -notmatch '^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
        Write-Host ("FAIL  tag '{0}' is not vX.Y.Z[-suffix]" -f $Tag) -ForegroundColor Red
        $failures += "tag format"
    }
    elseif ($Tag -ne "v$expected") {
        Write-Host ("FAIL  tag '{0}' does not match version v{1}" -f $Tag, $expected) -ForegroundColor Red
        $failures += "tag/version"
    }
    else {
        Write-Host ("ok    tag {0} matches the manifests" -f $Tag)
    }
}

if ($RequireChangelog) {
    $text = Read-Text $ChangelogPath
    $pattern = '(?ms)^## \[' + [regex]::Escape($expected) + '\][^\n]*\n(.*?)(?=^## |\z)'
    $match = [regex]::Match($text, $pattern)
    $hasContent = $false
    if ($match.Success) {
        $content = @($match.Groups[1].Value -split "`n" | Where-Object {
                $_.Trim() -and -not $_.TrimStart().StartsWith("#")
            })
        $hasContent = $content.Count -gt 0
    }
    if ($hasContent) {
        Write-Host ("ok    CHANGELOG.md has a non-empty [{0}] section" -f $expected)
    }
    else {
        Write-Host ("FAIL  CHANGELOG.md has no non-empty [{0}] section" -f $expected) -ForegroundColor Red
        $failures += "CHANGELOG"
    }
}

if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host ("{0} version check(s) failed" -f $failures.Count) -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host ("all version manifests agree on {0}" -f $expected) -ForegroundColor Green
