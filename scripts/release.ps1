[CmdletBinding()]
param(
    # Product version to release, e.g. 2.1.0.
    [Parameter(Mandatory)][string]$Version,

    # Release date written to CHANGELOG.md.
    [string]$Date = (Get-Date -Format "yyyy-MM-dd"),

    # Optional subtitle for the CHANGELOG heading, e.g. "schema-driven content hints".
    [string]$Title,

    # Commit and tag, but do not push.
    [switch]$NoPush,

    # Tag the current HEAD instead of bumping and committing (used after a fix).
    [switch]$TagOnly,

    # With -TagOnly: move an existing local/remote tag to HEAD.
    [switch]$Force,

    [string]$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Git {
    param([Parameter(Mandatory)][string[]]$Arguments)

    $output = & git -C $RepoRoot @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw ("git {0} failed:`n{1}" -f ($Arguments -join " "), ($output -join "`n"))
    }
    return (($output | Out-String).Trim())
}

function Test-GitHubRelease {
    param([Parameter(Mandatory)][string]$Tag)

    try {
        & gh release view $Tag --json tagName 2>$null | Out-Null
        return ($LASTEXITCODE -eq 0)
    }
    catch {
        return $false
    }
}

function Get-RepositorySlug {
    $url = Invoke-Git @("remote", "get-url", "origin")
    if ($url -match 'github\.com[:/](?<slug>[^/]+/[^/]+?)(\.git)?$') {
        return $Matches["slug"]
    }
    return $null
}

function Update-Changelog {
    param(
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][string]$Date,
        [string]$Title
    )

    $path = Join-Path $RepoRoot "CHANGELOG.md"
    $text = [System.IO.File]::ReadAllText($path)
    $headings = [regex]::Matches($text, '(?m)^## \[Unreleased\][^\n]*$')
    if ($headings.Count -ne 1) {
        throw "expected exactly one '## [Unreleased]' heading in CHANGELOG.md, found $($headings.Count)"
    }

    $heading = $headings[0]
    $after = $text.Substring($heading.Index + $heading.Length)
    $next = [regex]::Match($after, '(?m)^## ')
    $body = if ($next.Success) { $after.Substring(0, $next.Index) } else { $after }
    $entries = @($body -split "`n" | Where-Object {
            $_.Trim() -and -not $_.TrimStart().StartsWith("#")
        })
    if ($entries.Count -eq 0) {
        throw "the Unreleased section is empty; add entries before releasing"
    }

    $subtitle = if ($Title) { " — $Title" } else { "" }
    $replacement = "## [Unreleased]`n`n## [$Version]$subtitle - $Date"
    $updated = $text.Substring(0, $heading.Index) + $replacement + $after
    [System.IO.File]::WriteAllText($path, $updated, [System.Text.UTF8Encoding]::new($false))
    Write-Host "finalized CHANGELOG.md for $Version ($Date)"
}

if ($Force -and -not $TagOnly) {
    throw "-Force is only valid together with -TagOnly"
}
if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
    throw "invalid version '$Version'; expected X.Y.Z or X.Y.Z-suffix"
}
if ($Date -notmatch '^\d{4}-\d{2}-\d{2}$') {
    throw "invalid date '$Date'; expected yyyy-MM-dd"
}

$tag = "v$Version"

$status = Invoke-Git @("status", "--porcelain")
if ($status) {
    throw "working tree is not clean; commit or stash changes before releasing:`n$status"
}

$branch = Invoke-Git @("rev-parse", "--abbrev-ref", "HEAD")
if ($branch -ne "master") {
    throw "releases are cut from master, currently on '$branch'"
}

Invoke-Git @("fetch", "origin", "--tags", "--prune") | Out-Null
$head = Invoke-Git @("rev-parse", "HEAD")
$origin = Invoke-Git @("rev-parse", "origin/master")
if ($head -ne $origin) {
    throw "master is not in sync with origin/master; pull or push before releasing"
}

if (Test-GitHubRelease $tag) {
    throw "GitHub release $tag already exists; delete it before releasing again"
}

$localTag = Invoke-Git @("tag", "--list", $tag)
$remoteTag = Invoke-Git @("ls-remote", "--tags", "origin", "refs/tags/$tag")
$tagExists = [bool]$localTag -or [bool]$remoteTag
if ($tagExists -and -not ($TagOnly -and $Force)) {
    throw "tag $tag already exists; use -TagOnly -Force to move it after a fix"
}

if (-not $TagOnly) {
    & (Join-Path $PSScriptRoot "set-version.ps1") -Version $Version -RepoRoot $RepoRoot
    Update-Changelog -Version $Version -Date $Date -Title $Title

    & pwsh -NoProfile -File (Join-Path $PSScriptRoot "check-version.ps1") `
        -Version $Version -RequireChangelog -RepoRoot $RepoRoot
    if ($LASTEXITCODE -ne 0) {
        throw "version check failed; the release commit was not created"
    }

    $paths = @(
        "CHANGELOG.md",
        "README.md",
        "README.zh.md",
        "QUICKSTART.md",
        "QUICKSTART.zh.md",
        "docs/artifacts.md",
        "docs/artifacts.zh.md",
        "docs/testing.md",
        "docs/testing.zh.md",
        "docs/user-manual.md",
        "docs/user-manual.zh.md",
        "Nodara-Core/Cargo.toml",
        "Nodara-Core/Cargo.lock",
        "Nodara-Agent/Cargo.toml",
        "Nodara-Agent/Cargo.lock",
        "Nodara-Studio/package.json",
        "Nodara-Studio/package-lock.json",
        "Nodara-Studio/src-tauri/Cargo.toml",
        "Nodara-Studio/src-tauri/Cargo.lock",
        "Nodara-Studio/src-tauri/tauri.conf.json",
        "Nodara-Core/plugins/nodara-platform/manifest.json",
        "Nodara-Core/plugins/nodara-vision/manifest.json"
    )
    Invoke-Git (@("add", "--") + $paths) | Out-Null

    $staged = Invoke-Git @("diff", "--cached", "--name-only")
    if ($staged) {
        Invoke-Git @("commit", "-m", "chore(release): v$Version") | Out-Null
        Write-Host "committed: chore(release): v$Version"
    }
    else {
        Write-Host "no version or changelog changes to commit"
    }
}
else {
    & pwsh -NoProfile -File (Join-Path $PSScriptRoot "check-version.ps1") `
        -Version $Version -Tag $tag -RequireChangelog -RepoRoot $RepoRoot
    if ($LASTEXITCODE -ne 0) {
        throw "HEAD does not describe release $Version"
    }
}

if ($tagExists -and $Force) {
    if ($localTag) {
        Invoke-Git @("tag", "-d", $tag) | Out-Null
    }
    if ($remoteTag) {
        Invoke-Git @("push", "origin", ":refs/tags/$tag") | Out-Null
        Write-Host "deleted remote tag $tag"
    }
}

Invoke-Git @("tag", "-a", $tag, "-m", "Nodara $Version") | Out-Null
Write-Host "created tag $tag"

$slug = Get-RepositorySlug
if (-not $NoPush) {
    Invoke-Git @("push", "origin", "master", "refs/tags/$tag") | Out-Null
    Write-Host "pushed master and $tag" -ForegroundColor Green
    if ($slug) {
        Write-Host "release workflow: https://github.com/$slug/actions/workflows/release.yml"
        Write-Host "watch it with:   gh run watch --repo $slug"
    }
}
else {
    Write-Host "not pushed (-NoPush); push with: git push origin master refs/tags/$tag"
}

Write-Host ""
Write-Host "release $Version is prepared" -ForegroundColor Green
