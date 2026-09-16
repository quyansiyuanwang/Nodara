# Nodara Test and Acceptance Guide

> Chinese: [testing.zh.md](testing.zh.md)

This guide validates the prebuilt Windows x64 debug and release packages.

## Acceptance checklist

| ID | Check | Pass condition |
|---|---|---|
| A01 | Package integrity | ZIP hash and every entry in `SHA256SUMS.txt` match |
| A02 | Versions | All components report 2.0.0 and `build-info.json` names the expected commit |
| A03 | CLI smoke | Validate, simulate, run and `extensions --json` succeed |
| A04 | Plugins | Runtime reports 2 plugins and 19 node types |
| A05 | HTTP API | Health, plugins, extensions, node types, and schema endpoints respond |
| A06 | Studio | Desktop Studio connects and runs hello-world |
| A07 | Agent mock | A canned workflow plans and runs without an API key |
| A08 | Events/audit | Events and policy decisions are visible |
| A09 | Debug | Matching PDB files are present and executables run |
| A10 | Release | NSIS and MSI are present and the optimized app runs |
| A11 | Studio debugging UX | Step works from idle and advances one node at a time; node breakpoints pause before execution and survive Export; the region picker writes X/Y/width/height; screenshot and Log image previews render |

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

Require `status=ok`, 2 plugins, 0 failures, and 19 node types.

## Studio

With the runtime running, launch `nodara-studio.exe` and verify:

1. The status badge reports `19 node types · 2 plugin(s)`.
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
17. Close Notepad and run `examples/wait-for-window.json`; the Wait node should poll. Open Notepad before the timeout and confirm the workflow continues; a missing window should fail with `E_TIMEOUT`.
18. On an isolated test desktop, verify input timing: Keyboard `press shift` → Text `a` → Keyboard `release shift` produces an uppercase `A`; Mouse `drag` accepts optional start coordinates, destination coordinates and duration and moves smoothly.
19. Keep the target window in the background and send text plus client-coordinate mouse messages with `background=true` and a title/process selector. A message-capable control should receive them without changing focus or moving the real cursor; record unsupported applications as a compatibility limitation.
20. The Audit tab shows capability and node records.
21. Open `examples/breakpoint-debug.json`; the Calculate node shows a breakpoint marker, Run pauses before it with `nodes_executed=2`, and Resume or Step completes the workflow.
22. One of the NSIS/MSI installers installs, launches, connects, and uninstalls.
23. Marquee selection and node dragging do not open the floating quick card; a click without movement opens it. After a multi-selection, click any selected node to edit shared execution settings.
24. Type continuously in Inspector and Agent inputs while validation and session polling run; focus and caret position must remain stable. Plain clicks open the floating quick card, but Ctrl/Shift selection must not. Agent polling must also preserve button focus, expanded details and scroll position.
25. Drop edges on node bodies to verify unique inference and ambiguous-port choice. Hold Alt to create control and data edges together, including partial success. Switch all six themes, override node/edge colors, export/import the workflow and verify the overrides survive.
26. Verify Agent Provider Profiles use Windows Credential Manager, model output streams token by token, Stop cancels only generation, diffs and Trace render, and Audit shows live/history summary, graph, timeline and linked details.
27. Run a capture workflow, then send a follow-up Agent goal. Confirm the next model turn includes `node_started` inputs, `node_finished` outputs, transferred edge values and the screenshot as a native image attachment; automatic repair must use the same evidence.

## Agent

```powershell
.\nodara-agent.exe capabilities
$mock = '{"schema_version":"2.1","id":"wf.test","metadata":{"name":"Test"},"nodes":[{"id":"start","type":"core.Start"},{"id":"log","type":"core.Log","config":{"message":"agent ok"}},{"id":"end","type":"core.End"}],"edges":[{"id":"e1","kind":"control","source":"start","target":"log"},{"id":"e2","kind":"control","source":"log","target":"end"}]}'
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
