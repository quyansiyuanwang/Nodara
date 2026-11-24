[CmdletBinding()]
param(
    # Portable archive: .zip on Windows, .tar.gz on macOS and Linux.
    [Alias("ArchivePath")]
    [Parameter(Mandatory)][string]$ZipPath,
    [Parameter(Mandatory)][string]$Version,
    # Optional full commit SHA; build-info.json must start with it.
    [string]$Commit,
    [switch]$SkipRuntime,
    [string]$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ExecutableSuffix = if ($IsWindows) { ".exe" } else { "" }
$Platform = if ($IsWindows) { "windows" } elseif ($IsMacOS) { "macos" } else { "linux" }

function Assert-True {
    param(
        [Parameter(Mandatory)][bool]$Condition,
        [Parameter(Mandatory)][string]$Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    try {
        return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
}

function Get-MsiProductVersion {
    param([Parameter(Mandatory)][string]$Path)

    try {
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $database = $installer.GetType().InvokeMember(
            "OpenDatabase", "InvokeMethod", $null, $installer, @($Path, 0)
        )
        $view = $database.GetType().InvokeMember(
            "OpenView", "InvokeMethod", $null, $database,
            @("SELECT Value FROM Property WHERE Property='ProductVersion'")
        )
        $view.GetType().InvokeMember("Execute", "InvokeMethod", $null, $view, $null) | Out-Null
        $record = $view.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $view, $null)
        if ($null -eq $record) {
            return $null
        }
        return [string]$record.GetType().InvokeMember("StringData", "GetProperty", $null, $record, @(1))
    }
    catch {
        return $null
    }
}

function Invoke-Cli {
    param(
        [Parameter(Mandatory)][string]$Exe,
        [Parameter(Mandatory)][string[]]$Arguments,
        [Parameter(Mandatory)][string]$Label
    )

    $output = & $Exe @Arguments 2>&1
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        $output | ForEach-Object { Write-Host "    $_" }
        throw "nodara-cli $Label failed with exit code $code"
    }
    Write-Host ("ok    cli {0}" -f $Label)
}

function Stop-ProcessTree {
    param([System.Diagnostics.Process]$Process)

    if ($null -eq $Process -or $Process.HasExited) {
        return
    }
    if ($IsWindows) {
        & taskkill.exe /PID $Process.Id /T /F 2>$null | Out-Null
    }
    else {
        & pkill -P $Process.Id 2>$null | Out-Null
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 500
    if (-not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    }
}

$ArchivePath = [System.IO.Path]::GetFullPath($ZipPath)
Assert-True (Test-Path -LiteralPath $ArchivePath -PathType Leaf) "missing package: $ArchivePath"

$archiveName = [System.IO.Path]::GetFileName($ArchivePath)
$isTar = $archiveName.EndsWith(".tar.gz") -or $archiveName.EndsWith(".tgz")
$tempRoot = Join-Path $RepoRoot ("artifacts/tmp/verify-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
$runtime = $null
$previousPort = $env:NODARA_RUNTIME_PORT

try {
    # --- checksum ---------------------------------------------------------
    $checksumPath = "$ArchivePath.sha256"
    Assert-True (Test-Path -LiteralPath $checksumPath -PathType Leaf) "missing checksum file: $checksumPath"
    $checksumLine = (Get-Content -LiteralPath $checksumPath -Raw).Trim()
    $expectedHash = ($checksumLine -split '\s+')[0].ToLowerInvariant()
    $actualHash = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert-True ($actualHash -eq $expectedHash) "archive hash mismatch: $actualHash != $expectedHash"
    Write-Host ("ok    archive sha256 {0}" -f $actualHash)

    # --- extract ----------------------------------------------------------
    New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    if ($isTar) {
        & tar -xzf $ArchivePath -C $tempRoot
        Assert-True ($LASTEXITCODE -eq 0) "tar failed with exit code $LASTEXITCODE"
    }
    else {
        Expand-Archive -LiteralPath $ArchivePath -DestinationPath $tempRoot -Force
    }
    $packageDir = $null
    $candidates = @(Get-ChildItem -LiteralPath $tempRoot -Directory)
    Assert-True ($candidates.Count -eq 1) "expected a single package directory in the archive"
    $packageDir = $candidates[0].FullName

    # --- staged files -----------------------------------------------------
    $sumsPath = Join-Path $packageDir "SHA256SUMS.txt"
    Assert-True (Test-Path -LiteralPath $sumsPath -PathType Leaf) "missing SHA256SUMS.txt"
    $bad = @()
    foreach ($line in Get-Content -LiteralPath $sumsPath) {
        if (-not $line.Trim()) { continue }
        if ($line -notmatch '^([0-9a-fA-F]{64})  (.+)$') {
            $bad += "malformed line: $line"
            continue
        }
        $expectedFileHash = $Matches[1].ToLowerInvariant()
        $relative = $Matches[2]
        $file = Join-Path $packageDir $relative
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
            $bad += "$relative (missing)"
            continue
        }
        $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -ne $expectedFileHash) {
            $bad += "$relative (hash mismatch)"
        }
    }
    Assert-True ($bad.Count -eq 0) ("SHA256SUMS.txt verification failed:`n  " + ($bad -join "`n  "))
    Write-Host ("ok    SHA256SUMS.txt covers {0} file(s)" -f @(Get-Content -LiteralPath $sumsPath | Where-Object { $_.Trim() }).Count)

    $required = @(
        "nodara-cli$ExecutableSuffix",
        "nodara-runtime$ExecutableSuffix",
        "nodara-agent$ExecutableSuffix",
        "nodara-studio$ExecutableSuffix",
        "studio-web/index.html",
        "build-info.json",
        "SHA256SUMS.txt",
        "plugins/nodara-platform/manifest.json",
        "plugins/nodara-platform/nodara-platform-plugin$ExecutableSuffix",
        "plugins/nodara-vision/manifest.json",
        "plugins/nodara-vision/nodara-vision-plugin$ExecutableSuffix",
        "schema/workflow.schema.json",
        "examples/hello-world.json",
        "examples/screenshot-mail.json",
        "docs/testing.md",
        "docs/artifacts.md",
        "docs/user-manual.md"
    )
    $missing = @($required | Where-Object { -not (Test-Path -LiteralPath (Join-Path $packageDir $_)) })
    Assert-True ($missing.Count -eq 0) ("package is missing: " + ($missing -join ", "))
    Write-Host "ok    required entrypoints are present"

    # --- build metadata ---------------------------------------------------
    $info = (Get-Content -LiteralPath (Join-Path $packageDir "build-info.json") -Raw) | ConvertFrom-Json
    Assert-True ($info.product -eq "Nodara") "build-info.json names '$($info.product)', expected Nodara"
    Assert-True ($info.version -eq $Version) "build-info.json names version '$($info.version)', expected $Version"
    Assert-True ($info.configuration -eq "Release") "build-info.json configuration is '$($info.configuration)', expected Release"
    Assert-True ($info.platform -eq $Platform) "build-info.json platform is '$($info.platform)', expected $Platform"
    if ($Commit) {
        Assert-True ($Commit.StartsWith([string]$info.source_commit, [System.StringComparison]::OrdinalIgnoreCase)) `
            "build-info.json commit '$($info.source_commit)' does not match $Commit"
    }
    $installers = @($info.installers | Where-Object { $_ })
    foreach ($installer in $installers) {
        $installerPath = Join-Path $packageDir "installers/$installer"
        Assert-True (Test-Path -LiteralPath $installerPath) `
            "build-info.json lists a missing installer: $installer"
        if ($IsWindows -and $installerPath.EndsWith(".exe")) {
            $reported = (Get-Item -LiteralPath $installerPath).VersionInfo.ProductVersion
            Assert-True ($reported -eq $Version) `
                "installer $installer reports version $reported, expected $Version"
        }
        if ($IsWindows -and $installerPath.EndsWith(".msi")) {
            $reported = Get-MsiProductVersion -Path $installerPath
            Assert-True ($reported -eq $Version) `
                "installer $installer reports version $reported, expected $Version"
        }
    }
    if ($installers.Count -gt 0) {
        Write-Host ("ok    {0} installer(s) match version {1}" -f $installers.Count, $Version)
    }
    Write-Host ("ok    build-info.json {0} / {1} / {2} / {3} / {4}" -f $info.version, $info.configuration, $info.platform, $info.arch, $info.source_commit)

    # --- CLI --------------------------------------------------------------
    $cli = Join-Path $packageDir "nodara-cli$ExecutableSuffix"
    $versionOutput = (& $cli --version 2>&1 | Out-String).Trim()
    Assert-True ($LASTEXITCODE -eq 0) "nodara-cli --version failed"
    Assert-True ($versionOutput -match [regex]::Escape($Version)) "nodara-cli reports '$versionOutput', expected $Version"
    Write-Host ("ok    cli version {0}" -f $versionOutput)

    Push-Location $packageDir
    try {
        Invoke-Cli $cli @("validate", "examples/hello-world.json") "validate hello-world"
        Invoke-Cli $cli @("simulate", "examples/hello-world.json") "simulate hello-world"
        Invoke-Cli $cli @("run", "examples/delayed-log.json") "run delayed-log"
        Invoke-Cli $cli @("run", "examples/screenshot-mail.json", "--plugin-dir", "plugins", "--quiet") "run screenshot-mail through platform plugin"
        Invoke-Cli $cli @("validate", "examples/branching.json", "--json") "validate branching --json"
        Invoke-Cli $cli @("inspect", "examples/hello-world.json", "--json") "inspect hello-world --json"
        Invoke-Cli $cli @("extensions", "--json") "extensions --json"
    }
    finally {
        Pop-Location
    }

    # --- runtime and plugins ---------------------------------------------
    if (-not $SkipRuntime) {
        $port = Get-FreePort
        $env:NODARA_RUNTIME_PORT = [string]$port
        $stdoutPath = Join-Path $tempRoot "runtime.stdout.log"
        $stderrPath = Join-Path $tempRoot "runtime.stderr.log"
        $startArguments = @{
            FilePath               = Join-Path $packageDir "nodara-runtime$ExecutableSuffix"
            WorkingDirectory       = $packageDir
            PassThru               = $true
            RedirectStandardOutput = $stdoutPath
            RedirectStandardError  = $stderrPath
        }
        if ($IsWindows) {
            $startArguments.WindowStyle = "Hidden"
        }
        $runtime = Start-Process @startArguments

        $base = "http://127.0.0.1:$port/api/v1"
        $deadline = (Get-Date).AddSeconds(90)
        $health = $null
        while ((Get-Date) -lt $deadline) {
            if ($runtime.HasExited) { break }
            try {
                $health = Invoke-RestMethod -Uri "$base/health" -TimeoutSec 2 -NoProxy
                break
            }
            catch {
                Start-Sleep -Milliseconds 500
            }
        }

        if ($null -eq $health) {
            $stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { "" }
            $stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { "" }
            throw "runtime did not answer /api/v1/health on port $port`n--- stdout ---`n$stdout`n--- stderr ---`n$stderr"
        }

        Assert-True ($health.status -eq "ok") "runtime health status is '$($health.status)'"
        Assert-True ([int]$health.plugins -eq 2) "runtime reports $($health.plugins) plugin(s), expected 2"
        Assert-True ([int]$health.node_types -eq 21) "runtime reports $($health.node_types) node type(s), expected 21"
        Write-Host ("ok    runtime health: {0} node types, {1} plugins" -f $health.node_types, $health.plugins)

        $plugins = Invoke-RestMethod -Uri "$base/plugins" -TimeoutSec 5 -NoProxy
        $pluginFailures = @($plugins.failures)
        Assert-True ($pluginFailures.Count -eq 0) ("plugin failures: " + ($pluginFailures | ConvertTo-Json -Compress))
        Assert-True (@($plugins.plugins).Count -eq 2) "runtime lists $(@($plugins.plugins).Count) plugin(s), expected 2"

        $extensions = Invoke-RestMethod -Uri "$base/extensions" -TimeoutSec 5 -NoProxy
        Assert-True (@($extensions.extensions).Count -gt 0) "runtime lists no extensions"
        Write-Host ("ok    runtime extensions: {0}" -f @($extensions.extensions).Count)

        $schema = Invoke-RestMethod -Uri "$base/schema/workflow" -TimeoutSec 5 -NoProxy
        Assert-True ($null -ne $schema.properties) "workflow schema has no properties"
        Write-Host "ok    workflow schema endpoint responds"
    }
    else {
        Write-Host "skipped runtime smoke test (-SkipRuntime)"
    }

    Write-Host ""
    Write-Host ("package verified: {0}" -f $archiveName) -ForegroundColor Green
}
finally {
    if ($null -ne $runtime) {
        Stop-ProcessTree -Process $runtime
    }

    if ($null -ne $previousPort) {
        $env:NODARA_RUNTIME_PORT = $previousPort
    }
    else {
        Remove-Item Env:\NODARA_RUNTIME_PORT -ErrorAction SilentlyContinue
    }

    for ($attempt = 0; $attempt -lt 3; $attempt++) {
        try {
            if (Test-Path -LiteralPath $tempRoot) {
                Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction Stop
            }
            break
        }
        catch {
            Start-Sleep -Milliseconds 500
        }
    }
}
