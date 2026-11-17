[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,
    [string]$OutputPath,
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

$docs = "https://github.com/$Repository/blob/master/docs"
$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add($body)
$lines.Add("")
$lines.Add("## Installation")
$lines.Add("")
$lines.Add("| Asset | Contents |")
$lines.Add("|---|---|")
$lines.Add(('| `Nodara-{0}-windows-x86_64.zip` | Portable package: CLI, runtime, Agent, Studio, both official plugins, examples, schemas and documentation. |' -f $Version))
$lines.Add(('| `Nodara-Studio-{0}-x64-setup.exe` | NSIS installer for Nodara Studio (bundles the runtime and both plugins). |' -f $Version))
$lines.Add(('| `Nodara-Studio-{0}-x64.msi` | MSI installer for Nodara Studio (bundles the runtime and both plugins). |' -f $Version))
$lines.Add("")
$lines.Add("Requirements: Windows 10/11 x64. The portable package needs no Rust, Node.js or Visual Studio.")
$lines.Add("")
$lines.Add("## Verify the download")
$lines.Add("")
$lines.Add('```powershell')
$lines.Add(('(Get-FileHash .\Nodara-{0}-windows-x86_64.zip -Algorithm SHA256).Hash.ToLowerInvariant()' -f $Version))
$lines.Add(('Get-Content .\Nodara-{0}-windows-x86_64.zip.sha256' -f $Version))
$lines.Add('```')
$lines.Add("")
$lines.Add("The two hashes must match. `SHA256SUMS.txt` inside the package covers every staged file, and `SHA256SUMS.txt` on this release covers the published assets.")
$lines.Add("")
$lines.Add("## Notes")
$lines.Add("")
$lines.Add("The binaries are not code-signed, so Windows SmartScreen may warn on first launch. Verify the checksum, then choose **More info** → **Run anyway**.")
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
