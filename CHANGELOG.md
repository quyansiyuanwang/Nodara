# Changelog

All notable changes to RecognizerFramework will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - 2024-09-11

### Added

#### Visual Editor (Studio)
- Browser-based graph editor with drag-and-drop interface
- Visual node connections with SVG rendering
- Real-time property editor with validation
- Execution controls (Run, Pause, Resume, Step, Cancel)
- Import/Export workflow JSON files
- Layout persistence in workflow metadata
- Zero-dependency implementation (no npm, no build step)

#### AI Agent Integration
- `AgentController` for bounded observe-plan-act loops
- OpenAI-compatible adapter with provider-neutral design
- Tool registry with 5 built-in tools (Create, Validate, Simulate, Inspect, Repair)
- Authorization policy with risk levels (Safe, Moderate, High)
- Budget enforcement (max steps, time, tokens)
- Structured output with schema validation
- Complete audit trail for all tool calls

#### Windows Platform Support
- Window enumeration with title/className/process filters (`Window.Find`)
- Window focus control via `SetForegroundWindow`
- Background input routing via `SendMessageW`
- Keyboard input with virtual key codes and chord support
- Mouse input (click, move, drag)
- Text input with proper key sequence generation
- Desktop screen capture (`Desktop.Capture`)
- Window capture with occlusion control (`Window.Capture`)
- Clipboard read/write operations (`System.Paste`)

#### Core Engine
- Deterministic graph execution engine
- Event streaming with `RunEvent` for UI integration
- Execution control (pause, resume, step, cancel)
- Variable scoping and binding
- Retry logic with exponential backoff
- Hook system (before/after node execution)
- Dependency resolution with cycle detection

#### Schema & Validation
- Workflow v2 JSON Schema with strict validation
- v1-to-v2 migration tooling
- Overload inheritance with deep-merge semantics
- Schema-driven node configuration

#### Vision & OCR
- Grayscale template matching (no OpenCV dependency)
- Provider injection pattern for OCR
- Optional Tesseract integration
- Remote OCR transport protocol
- Model package manifest with checksum verification

#### CLI
- `validate` - Validate workflow schema
- `simulate` - Dry-run workflow without side effects
- `run` - Execute workflow with permission control
- `migrate` - Convert v1 workflows to v2

#### Documentation
- Complete README with feature overview
- Quick start guide (5-minute setup)
- Contributing guidelines
- API documentation structure
- Example workflows

### Changed
- Complete rewrite from Python to Rust
- Event-driven architecture replacing polling
- Schema-first design with JSON Schema validation
- Provider-neutral AI integration (was hardcoded OpenAI)
- Injected transport pattern (no HTTP in core library)

### Improved
- **Performance**: 10x+ faster execution vs Python
- **Memory Safety**: Rust guarantees, no segfaults
- **Type Safety**: Compile-time validation
- **Error Handling**: Result types with proper error propagation
- **Test Coverage**: 94 comprehensive tests

### Security
- Explicit permission model for high-risk operations
- Tool authorization for AI agent
- Budget limits prevent runaway execution
- Safe Rust for memory safety
- Proper resource cleanup in unsafe blocks

### Platform Compatibility
- Windows 10/11: Full support ✅
- macOS: Planned (interface defined)
- Linux: Planned (interface defined)

### Breaking Changes from v1
- New JSON schema (v2) - use migration tool
- Different CLI command syntax
- Capability-based permission model
- Expression evaluation improvements (right-associative power)

## [1.0.0] - Legacy Python Implementation

### Features
- Python-based workflow engine
- Basic Windows automation
- Template matching with OpenCV
- Simple workflow execution

---

For upgrade instructions, see [docs/migration-v1-to-v2.md](docs/migration-v1-to-v2.md).

[2.0.0]: https://github.com/yourusername/RecognizerFramework/releases/tag/v2.0.0
[1.0.0]: https://github.com/yourusername/RecognizerFramework/releases/tag/v1.0.0
