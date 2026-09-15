# Nodara Test and Acceptance Guide

> Chinese: [testing.zh.md](testing.zh.md)

This guide validates the prebuilt Windows x64 debug and release packages.

## Acceptance checklist

| ID | Check | Pass condition |
|---|---|---|
| A01 | Package integrity | ZIP hash and every entry in `SHA256SUMS.txt` match |
| A02 | Versions | All components report 2.0.0 and `build-info.json` names the expected commit |
| A03 | CLI smoke | Validate, simulate, run and `extensions --json` succeed |
| A04 | Plugins | Runtime reports 2 plugins and 17 node types |
| A05 | HTTP API | Health, plugins, extensions, node types, and schema endpoints respond |
| A06 | Studio | Desktop Studio connects and runs hello-world |
| A07 | Agent mock | A canned workflow plans and runs without an API key |
| A08 | Events/audit | Events and policy decisions are visible |
| A09 | Debug | Matching PDB files are present and executables run |
| A10 | Release | NSIS and MSI are present and the optimized app runs |
| A11 | Studio debugging UX | Step works from idle and advances one node at a time; the region picker writes X/Y/width/height; screenshot and Log image previews render |

## Integrity

```powershell
$zip = ".\artifacts\release\Nodara-2.0.0-windows-x86_64.zip"
$expected = (Get-Content "$zip.sha256").Split()[0]
$actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "ZIP hash mismatch" }

Expand-Archive $zip -DestinationPath .\verify-release -Force
Set-Location .\verify-release\Nodara-2.0.0-windows-x86_64

$bad = @()
Get-Content .\SHA256SUMS.txt | ForEach-Object {
  $parts = $_ -split '  ', 2
  $hash = (Get-FileHash -LiteralPath $parts[1] -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($hash -ne $parts[0]) { $bad += $parts[1] }
}
if ($bad.Count) { throw "hash mismatch: $($bad -join ', ')" }
```

## CLI

```powershell
.\nodara-cli.exe --version
.\nodara-cli.exe validate .\examples\hello-world.json
.\nodara-cli.exe simulate .\examples\hello-world.json
.\nodara-cli.exe run .\examples\delayed-log.json
.\nodara-cli.exe validate .\examples\branching.json --json
.\nodara-cli.exe inspect .\examples\hello-world.json --json
```

Check exit codes, structured JSON, and that simulate does not execute a node.

## Runtime and plugins

```powershell
.\nodara-runtime.exe
```

In another terminal:

```powershell
$base = "http://127.0.0.1:8710/api/v1"
$health = Invoke-RestMethod "$base/health"
$plugins = Invoke-RestMethod "$base/plugins"
$types = Invoke-RestMethod "$base/node-types"

$health
$plugins.plugins | Select-Object id,version
"node types: $($types.node_types.Count)"
"plugin failures: $($plugins.failures.Count)"
```

Require `status=ok`, 2 plugins, 0 failures, and 17 node types.

## Studio

With the runtime running, launch `nodara-studio.exe` and verify:

1. The status badge reports `17 node types · 2 plugin(s)`.
2. Hello-world imports with the `Start → Log → End` graph.
3. Validate and Run complete successfully.
4. The Events tab reaches `run_completed`.
5. Runs lists the execution and **Open** reloads its event stream.
6. **Variables…** accepts a temporary value, does not alter the workflow default, and rejects invalid JSON before Run.
7. **Extensions** lists the built-in registration and any loaded/discovered plugins with node counts and status.
8. `examples/capture-preview.json` displays the PNG artifact inline under its Capture event and exposes it through the run artifact API.
9. `examples/failure-branch.json` validates cleanly and its failure edge recovers from the failing Calculate node.
10. `examples/result-mapping.json` publishes its `out` port as `waited_ms` and the following Log interpolates it.
11. Pause, Resume, and Cancel work with a delayed workflow.
12. From idle, completed, failed or cancelled state, **Step** starts paused and executes one node; each later click increments `nodes_executed` by exactly one and returns the status to `paused`.
13. Selecting `windows.Desktop.Capture` and choosing **Select screen region** hides and restores Studio; the captured image does not contain the picker dialog, and applying a drag writes matching X/Y/width/height values.
14. A `core.Log` message containing artifact JSON, such as `{{screenshot}}`, also renders the image inline.
15. `examples/system-command.json` runs successfully; the Command node's `out` contains stdout and the following Log interpolates it. Verify non-zero exit and cancellation/timeout behavior against the documented outputs.
16. With Notepad open, `examples/window-find.json` finds it using both the `Notepad` title and `notepad.exe` process filters; the output contains the resolved process name and `visible=true`.
17. On an isolated test desktop, verify input timing: Keyboard `press shift` → Text `a` → Keyboard `release shift` produces an uppercase `A`; Mouse `drag` accepts optional start coordinates, destination coordinates and duration and moves smoothly.
18. The Audit tab shows capability and node records.
19. One of the NSIS/MSI installers installs, launches, connects, and uninstalls.

## Agent

```powershell
.\nodara-agent.exe capabilities
$mock = '{"schema_version":"2.0","id":"wf.test","metadata":{"name":"Test"},"nodes":[{"id":"start","type":"core.Start"},{"id":"log","type":"core.Log","config":{"message":"agent ok"}},{"id":"end","type":"core.End"}],"edges":[{"id":"e1","source":"start","target":"log"},{"id":"e2","source":"log","target":"end"}]}'
.\nodara-agent.exe plan "log agent ok" --mock $mock --trace .\trace.jsonl
.\nodara-agent.exe replay .\trace.jsonl
.\nodara-agent.exe run "log agent ok" --mock $mock --report
```

Test `--safe` and `--allow` refusal paths. Only test privileged desktop actions on a
dedicated machine after reviewing `simulate`.

## Debug/release comparison

Debug includes PDB files and uses `opt-level=1`; release is stripped, uses
`opt-level=3` with LTO, and adds NSIS/MSI installers. Functional results must
match. Performance measurements must use release.

## Record template

```text
Build: Nodara 2.0.0 / debug or release / commit
Environment: Windows, DPI, Defender, WebView2, VC++ version
Results: A01-A10 pass or fail
Failed step, expected result, actual result:
Attachments: build-info.json, SHA256SUMS.txt, logs, trace/audit
```

Stop all processes and remove the temporary extraction directory after testing.
