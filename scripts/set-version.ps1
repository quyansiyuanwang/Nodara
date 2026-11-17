[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,
    [string]$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
    throw "invalid version '$Version'; expected X.Y.Z or X.Y.Z-suffix"
}

$CoreRoot = Join-Path $RepoRoot "Nodara-Core"
$AgentRoot = Join-Path $RepoRoot "Nodara-Agent"
$StudioRoot = Join-Path $RepoRoot "Nodara-Studio"
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

function Update-File {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][string]$Replacement,
        [switch]$All
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "missing file: $Path"
    }

    $text = [System.IO.File]::ReadAllText($Path)
    $regex = [regex]::new($Pattern)
    $updated = if ($All) {
        $regex.Replace($text, $Replacement)
    }
    else {
        $regex.Replace($text, $Replacement, 1)
    }

    if ($updated -ne $text) {
        [System.IO.File]::WriteAllText($Path, $updated, $Utf8NoBom)
        Write-Host "updated $Path"
    }
}

function Get-ProductVersion {
    $path = Join-Path $CoreRoot "Cargo.toml"
    $text = [System.IO.File]::ReadAllText($path)
    $match = [regex]::Match($text, '(?ms)^\[workspace\.package\][^\[]*?^version\s*=\s*"([^"]+)"')
    if (-not $match.Success) {
        throw "could not read the product version from $path"
    }
    return $match.Groups[1].Value
}

function Update-Lockfile {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string[]]$Packages
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "missing file: $Path"
    }

    $lines = [System.IO.File]::ReadAllLines($Path)
    $changed = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match '^name = "(.+)"$' -and $Packages -contains $Matches[1]) {
            $limit = [Math]::Min($i + 4, $lines.Count)
            for ($j = $i + 1; $j -lt $limit; $j++) {
                if ($lines[$j] -match '^version = "(.+)"$') {
                    if ($Matches[1] -ne $Version) {
                        $lines[$j] = 'version = "' + $Version + '"'
                        $changed = $true
                    }
                    break
                }
            }
        }
    }

    if ($changed) {
        [System.IO.File]::WriteAllText($Path, (($lines -join "`n") + "`n"), $Utf8NoBom)
        Write-Host "updated $Path"
    }
}

$oldVersion = Get-ProductVersion

# `version` inside a TOML section, without touching unrelated tables.
$sectionVersion = '(?ms)^(\[' + '%SECTION%' + '\][^\[]*?^version\s*=\s*")[^"]+(")'
$jsonVersion = '(?m)^(\s*"version"\s*:\s*")[^"]+(")'

Update-File (Join-Path $CoreRoot "Cargo.toml") `
    ($sectionVersion.Replace('%SECTION%', 'workspace\.package')) `
    ('${1}' + $Version + '${2}')
Update-File (Join-Path $AgentRoot "Cargo.toml") `
    ($sectionVersion.Replace('%SECTION%', 'workspace\.package')) `
    ('${1}' + $Version + '${2}')
Update-File (Join-Path $StudioRoot "src-tauri/Cargo.toml") `
    ($sectionVersion.Replace('%SECTION%', 'package')) `
    ('${1}' + $Version + '${2}')

Update-File (Join-Path $StudioRoot "package.json") $jsonVersion ('${1}' + $Version + '${2}')
Update-File (Join-Path $StudioRoot "src-tauri/tauri.conf.json") $jsonVersion ('${1}' + $Version + '${2}')
Update-File (Join-Path $CoreRoot "plugins/nodara-platform/manifest.json") $jsonVersion ('${1}' + $Version + '${2}')
Update-File (Join-Path $CoreRoot "plugins/nodara-vision/manifest.json") $jsonVersion ('${1}' + $Version + '${2}')

# package-lock.json has a version for every dependency, so only the root object
# and the workspace entry may change.
Update-File (Join-Path $StudioRoot "package-lock.json") `
    '(?ms)^(\{\s*"name"\s*:\s*"nodara-studio"\s*,\s*"version"\s*:\s*")[^"]+(")' `
    ('${1}' + $Version + '${2}')
Update-File (Join-Path $StudioRoot "package-lock.json") `
    '(?ms)("":\s*\{\s*"name"\s*:\s*"nodara-studio"\s*,\s*"version"\s*:\s*")[^"]+(")' `
    ('${1}' + $Version + '${2}')

Update-Lockfile (Join-Path $CoreRoot "Cargo.lock") @(
    "nodara-schema", "nodara-core", "nodara-plugin", "nodara-runtime",
    "nodara-platform", "nodara-vision", "nodara-cli", "nodara-testkit"
)
Update-Lockfile (Join-Path $AgentRoot "Cargo.lock") @("nodara-agent", "nodara-schema")
Update-Lockfile (Join-Path $StudioRoot "src-tauri/Cargo.lock") @("nodara-studio")

# The acceptance and user guides quote the packaged version in their commands,
# so keep them in step with the manifests. CHANGELOG history, protocol examples
# and test fixtures are deliberately not in this list.
if ($oldVersion -ne $Version) {
    $documents = @(
        "README.md",
        "README.zh.md",
        "QUICKSTART.md",
        "QUICKSTART.zh.md",
        "docs/artifacts.md",
        "docs/artifacts.zh.md",
        "docs/testing.md",
        "docs/testing.zh.md",
        "docs/user-manual.md",
        "docs/user-manual.zh.md"
    )
    foreach ($document in $documents) {
        $path = Join-Path $RepoRoot $document
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            continue
        }
        $text = [System.IO.File]::ReadAllText($path)
        $updated = [regex]::Replace($text, [regex]::Escape($oldVersion), $Version)
        if ($updated -ne $text) {
            [System.IO.File]::WriteAllText($path, $updated, $Utf8NoBom)
            Write-Host "updated $path"
        }
    }
}

Write-Host ""
Write-Host "all manifests now declare $Version" -ForegroundColor Green
