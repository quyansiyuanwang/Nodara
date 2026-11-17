[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,
    [string]$OutputPath,
    # Directory containing the assets that will be attached to the release.
    [string]$AssetsPath,
    [string]$ChangelogPath = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../CHANGELOG.md")),
    [string]$Repository = "quyansiyuanwang/Nodara"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
    throw "invalid version '$Version'"
}
if (-not (Test-Path -LiteralPath $ChangelogPath -PathType Leaf)) {
    throw "missing changelog: $ChangelogPath"
}

$text = [System.IO.File]::ReadAllText($ChangelogPath)
$pattern = '(?ms)^## \[' + [regex]::Escape($Version) + '\][^\n]*\n(.*?)(?=^## |\z)'
$match = [regex]::Match($text, $pattern)
if (-not $match.Success) {
    throw "CHANGELOG.md has no '## [$Version]' section"
}

$body = $match.Groups[1].Value.Trim()
$content = @($body -split "`n" | Where-Object {
        $_.Trim() -and -not $_.TrimStart().StartsWith("#")
    })
if ($content.Count -eq 0) {
    throw "the '## [$Version]' section in CHANGELOG.md is empty"
}

function Get-AssetDescription {
    param([string]$Name)

    if ($Name -like "*-windows-x86_64.zip") { return "Windows x64 portable package" }
    if ($Name -like "*-windows-x86_64-debug.zip") { return "Windows x64 debug package" }
    if ($Name -like "*-linux-x86_64.tar.gz") { return "Linux x64 portable package" }
    if ($Name -like "*-macos-aarch64.tar.gz") { return "macOS Apple Silicon portable package" }
    if ($Name -like "*-macos-x86_64.tar.gz") { return "macOS Intel portable package" }
    if ($Name -like "*-x64-setup.exe") { return "Windows NSIS installer" }
    if ($Name -like "*-x64.msi") { return "Windows MSI installer" }
    if ($Name -like "*.deb") { return "Debian/Ubuntu installer" }
    if ($Name -like "*.AppImage") { return "Linux AppImage" }
    if ($Name -like "*.dmg") { return "macOS disk image" }
    if ($Name -like "*build-info.json") { return "Build metadata" }
    return $null
}

$assets = @()
if ($AssetsPath -and (Test-Path -LiteralPath $AssetsPath -PathType Container)) {
    $assets = @(Get-ChildItem -LiteralPath $AssetsPath -File |
        Where-Object { $_.Name -ne "release-notes.md" -and -not $_.Name.EndsWith(".sha256") } |
        Sort-Object Name |
        ForEach-Object {
            $description = Get-AssetDescription $_.Name
            if ($description) {
                [pscustomobject]@{ Name = $_.Name; Description = $description }
            }
        })
}

$docs = "https://github.com/$Repository/blob/master/docs"
$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add($body)
$lines.Add("")
$lines.Add("## Downloads")
$lines.Add("")
if ($assets.Count -gt 0) {
    $lines.Add("| Asset | Platform |")
    $lines.Add("|---|---|")
    foreach ($asset in $assets) {
        $lines.Add(('| `{0}` | {1} |' -f $asset.Name, $asset.Description))
    }
}
else {
    $lines.Add(('Windows, macOS and Linux packages are attached to this release as `Nodara-{0}-<platform>-<arch>.zip` or `.tar.gz`, with matching `.sha256` files and installers where the platform provides them.' -f $Version))
}
$lines.Add("")
$lines.Add("Requirements:")
$lines.Add("")
$lines.Add("- **Windows**: Windows 10/11 x64.")
$lines.Add("- **macOS**: macOS 12 or newer, Intel or Apple Silicon. The binaries are unsigned: right-click the app and choose **Open** the first time, or clear the quarantine attribute.")
$lines.Add("- **Linux**: x64 with X11 or XWayland for window automation. The Studio needs WebKitGTK 4.1; the CLI, runtime and Agent run on a headless host.")
$lines.Add("")
$lines.Add("## Verify the download")
$lines.Add("")
$lines.Add("Every archive has a matching `.sha256` file. On PowerShell:")
$lines.Add("")
$lines.Add('```powershell')
$lines.Add('Get-ChildItem .\Nodara-*.zip, .\Nodara-*.tar.gz | ForEach-Object {')
$lines.Add('  $expected = (Get-Content "$($_.FullName).sha256").Split()[0]')
$lines.Add('  $actual = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()')
$lines.Add('  if ($actual -ne $expected) { throw "hash mismatch: $($_.Name)" }')
$lines.Add('}')
$lines.Add('```')
$lines.Add("")
$lines.Add("On macOS and Linux: `shasum -a 256 -c <asset>.sha256` or `sha256sum -c <asset>.sha256`.")
$lines.Add("`SHA256SUMS.txt` inside each package covers every staged file.")
$lines.Add("")
$lines.Add("## Notes")
$lines.Add("")
$lines.Add("The binaries are not code-signed. Verify the checksum before running them.")
$lines.Add("")
$lines.Add("Documentation: [releasing]($docs/releasing.md) · [testing]($docs/testing.md) · [artifacts]($docs/artifacts.md) · [user manual]($docs/user-manual.md)")

$result = ($lines -join "`n") + "`n"

if ($OutputPath) {
    $parent = Split-Path -Parent $OutputPath
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    [System.IO.File]::WriteAllText(
        [System.IO.Path]::GetFullPath($OutputPath),
        $result,
        [System.Text.UTF8Encoding]::new($false)
    )
    Write-Host "wrote release notes to $OutputPath"
}
else {
    Write-Output $result
}
