# RecognizerFramework

A high-performance workflow automation framework with visual editor, AI-powered workflow generation, and cross-platform support.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

## Overview

RecognizerFramework is a modern automation platform built in Rust, featuring:

- 🎨 **Visual Workflow Editor** - Browser-based graph editor with drag-and-drop interface
- 🤖 **AI Integration** - Create and optimize workflows using natural language
- ⚡ **High Performance** - Rust implementation with deterministic execution
- 🖥️ **Windows Automation** - Complete input control, window management, and screen capture
- 👁️ **Vision & OCR** - Template matching and text recognition
- 📱 **Desktop Application** - Native desktop app built with Tauri

## Quick Start

### Prerequisites

- Rust 1.70+ ([install here](https://rustup.rs))
- Windows 10/11 (for platform features)

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/RecognizerFramework.git
cd RecognizerFramework

# Build the project
cd RecognizerFramework
cargo build --release

# Run tests
cargo test --workspace
```

### Run Visual Editor

```bash
# Start a local web server from project root
python -m http.server 4173

# Open in browser
# Navigate to: http://localhost:4173/RecognizerFramework/studio/
```

### Create Your First Workflow

**Using Visual Editor:**
1. Open Studio in browser
2. Drag "System.Log" from palette to canvas
3. Connect Start → System.Log → Exit
4. Configure node: `{"message": "Hello World!"}`
5. Click "▶ Run" to execute

**Using CLI:**
```bash
cd RecognizerFramework

# Validate workflow
cargo run -p rf-cli -- validate examples/hello-world.json

# Run workflow
cargo run -p rf-cli -- run examples/hello-world.json
```

## Features

### Visual Workflow Editor

Browser-based graph editor with:
- **Node palette** - Drag-and-drop interface
- **Visual connections** - Connect nodes with lines
- **Property editor** - Configure node parameters
- **Real-time validation** - Instant error feedback
- **Execution controls** - Run, pause, resume, step, cancel

### AI-Powered Workflow Creation

```rust
// Create workflows using natural language
let agent = AgentController::new(model, policy);
let workflow = agent.execute(
    "Create a workflow that logs a message and waits 2 seconds"
).await?;
```

**AI Features:**
- Natural language to workflow conversion
- Workflow validation and repair
- Tool authorization and safety controls
- Budget enforcement (steps, time, tokens)

### Windows Automation

Complete Windows platform support:
- **Window Management** - Find, focus, enumerate windows
- **Input Control** - Keyboard, mouse, text input
- **Screen Capture** - Desktop and window screenshots
- **Clipboard Operations** - Read/write clipboard
- **Background Input** - Send input to hidden windows

### Vision & OCR

Lightweight vision capabilities:
- Template matching without OpenCV dependency
- System OCR integration
- Tesseract support (optional)
- Remote OCR providers

## Architecture

```
RecognizerFramework/
├── crates/
│   ├── rf-schema/      # Workflow types and validation
│   ├── rf-core/        # Execution engine
│   ├── rf-platform/    # Platform adapters (Windows/macOS/Linux)
│   ├── rf-vision/      # Vision and OCR
│   ├── rf-agent/       # AI agent integration
│   └── rf-cli/         # Command-line interface
├── studio/             # Visual workflow editor
├── desktop/            # Tauri desktop application
├── schema/             # JSON schemas
└── examples/           # Example workflows
```

## Documentation

- [Quick Start Guide](QUICKSTART.md) - Get started in 5 minutes
- [Visual Editor Guide](RecognizerFramework/studio/README.md) - Studio documentation
- [API Documentation](docs/api.md) - Developer reference
- [Examples](examples/) - Sample workflows

## Examples

### Simple Automation

```json
{
  "version": 2,
  "nodes": [
    {"id": "start", "kind": "Start", "config": {}},
    {
      "id": "log",
      "kind": "System.Log",
      "config": {"message": "Hello World!"}
    },
    {
      "id": "wait",
      "kind": "System.Delay",
      "config": {"seconds": 2}
    }
  ],
  "edges": [
    {"from": "start", "to": "log"},
    {"from": "log", "to": "wait"},
    {"from": "wait", "to": "exit"}
  ]
}
```

### Window Automation

```json
{
  "id": "find-notepad",
  "kind": "Window.Find",
  "config": {
    "title": "Notepad",
    "exact": false
  }
}
```

More examples in [examples/](examples/) directory.

## CLI Usage

```bash
# Validate workflow
cargo run -p rf-cli -- validate workflow.json

# Simulate (dry-run)
cargo run -p rf-cli -- simulate workflow.json

# Execute with permissions
cargo run -p rf-cli -- run workflow.json \
  --allow window.enumerate,input.control \
  --allow-shell

# Migrate from v1
cargo run -p rf-cli -- migrate legacy.json new.json
```

## Development

### Build

```bash
cd RecognizerFramework
cargo build --release
```

### Test

```bash
# Run all tests
cargo test --workspace

# Run specific test
cargo test -p rf-platform test_name

# With output
cargo test -- --nocapture
```

### Format & Lint

```bash
cargo fmt --all
cargo clippy --all-targets
```

### Desktop App

```bash
cd RecognizerFramework/desktop
npm install
npm run tauri dev
npm run tauri build
```

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

This project is licensed under the MIT License - see [LICENSE](LICENSE) for details.

## Support

- **Documentation**: [docs/](docs/)
- **Examples**: [examples/](examples/)
- **Issues**: [GitHub Issues](https://github.com/yourusername/RecognizerFramework/issues)

## Acknowledgments

Built with ❤️ using Rust, designed for reliability and performance.

---

**Status**: Production Ready | **Latest Version**: 2.0.0 | **Platform**: Windows (macOS/Linux planned)
