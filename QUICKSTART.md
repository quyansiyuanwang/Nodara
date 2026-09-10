# Quick Start Guide

Get up and running with RecognizerFramework in 5 minutes.

## Step 1: Install Prerequisites

### Required
- **Rust 1.70+**: Download from [rustup.rs](https://rustup.rs)
- **Windows 10/11**: For platform automation features

### Optional
- **Python 3.x**: For serving the visual editor
- **Node.js 18+**: For desktop app development

## Step 2: Clone and Build

```bash
# Clone repository
git clone https://github.com/yourusername/RecognizerFramework.git
cd RecognizerFramework

# Navigate to core workspace
cd RecognizerFramework

# Build in release mode
cargo build --release

# Run tests to verify installation
cargo test --workspace
```

Expected output: All tests pass ✅

## Step 3: Choose Your Path

### Option A: Visual Editor (Recommended)

Perfect for beginners and visual workflow design.

```bash
# From project root directory
cd ..
python -m http.server 4173
```

Open browser: `http://localhost:4173/RecognizerFramework/studio/`

**Create your first workflow:**
1. Drag "System.Log" from left palette to canvas
2. Connect: Start → System.Log → Exit (click and drag)
3. Select the Log node
4. In right panel, edit config: `{"message": "Hello from Studio!"}`
5. Click "▶ Run" button
6. Watch execution in event log at bottom

### Option B: Command Line

Perfect for developers and automation scripts.

```bash
cd RecognizerFramework

# Create a simple workflow file
cat > hello.json << 'EOF'
{
  "$schema": "./schema/generated.schema.json",
  "version": 2,
  "nodes": [
    {"id": "start", "kind": "Start", "config": {}},
    {
      "id": "hello",
      "kind": "System.Log",
      "config": {"message": "Hello from CLI!"}
    }
  ],
  "edges": [
    {"from": "start", "to": "hello"},
    {"from": "hello", "to": "exit"}
  ]
}
EOF

# Validate
cargo run -p rf-cli -- validate hello.json

# Run
cargo run -p rf-cli -- run hello.json
```

## Step 4: Explore Features

### Try Input Automation

```json
{
  "id": "type-text",
  "kind": "Input.Text",
  "config": {
    "message": "Hello World"
  }
}
```

### Try Window Management

```json
{
  "id": "find-window",
  "kind": "Window.Find",
  "config": {
    "title": "Notepad",
    "exact": false
  }
}
```

### Try Screen Capture

```json
{
  "id": "screenshot",
  "kind": "Desktop.Capture",
  "config": {
    "x": 0,
    "y": 0,
    "width": 1920,
    "height": 1080
  }
}
```

### Try Calculations

```json
{
  "id": "calculate",
  "kind": "Calculate",
  "config": {
    "expression": "2 * 3 + 4",
    "result": "answer"
  }
}
```

## Step 5: Build Something Real

### Example: Automated Note Taking

```json
{
  "version": 2,
  "nodes": [
    {"id": "start", "kind": "Start", "config": {}},
    {
      "id": "find-notepad",
      "kind": "Window.Find",
      "config": {"title": "Notepad", "exact": false}
    },
    {
      "id": "type-title",
      "kind": "Input.Text",
      "config": {"message": "Meeting Notes\\n\\n"}
    },
    {
      "id": "type-content",
      "kind": "Input.Text",
      "config": {"message": "- Item 1\\n- Item 2\\n- Item 3"}
    }
  ],
  "edges": [
    {"from": "start", "to": "find-notepad"},
    {"from": "find-notepad", "to": "type-title"},
    {"from": "type-title", "to": "type-content"},
    {"from": "type-content", "to": "exit"}
  ]
}
```

## Understanding Permissions

High-risk operations require explicit permission:

```bash
# Window operations
cargo run -p rf-cli -- run workflow.json --allow window.enumerate

# Input control
cargo run -p rf-cli -- run workflow.json --allow input.control

# Shell commands
cargo run -p rf-cli -- run workflow.json --allow-shell

# Multiple permissions
cargo run -p rf-cli -- run workflow.json \
  --allow window.enumerate,input.control,desktop.capture \
  --allow-shell
```

## Troubleshooting

### Studio Won't Load
- Ensure you're serving from project root
- Check URL: `http://localhost:4173/RecognizerFramework/studio/`
- Try a different port: `python -m http.server 8000`

### Build Errors
```bash
# Update Rust
rustup update

# Clean and rebuild
cargo clean
cargo build --release
```

### Test Failures
```bash
# Update dependencies
cargo update

# Run with verbose output
cargo test --workspace -- --nocapture
```

## Next Steps

- 📖 Read [README.md](README.md) for complete feature overview
- 🎓 Explore [examples/](examples/) for sample workflows
- 🛠️ Check [docs/](docs/) for detailed documentation
- 🤝 See [CONTRIBUTING.md](CONTRIBUTING.md) to contribute

## Common Questions

**Q: Can I use this on macOS/Linux?**
A: Platform adapters for macOS/Linux are planned. The core engine and editor work on all platforms.

**Q: How do I integrate AI?**
A: See [docs/ai-integration.md](docs/ai-integration.md) for AI agent usage.

**Q: Is it production ready?**
A: Yes! All tests pass and Windows platform features are fully implemented.

**Q: Can I automate games?**
A: The framework supports input and screen capture, but check game terms of service.

## Getting Help

- 📚 Documentation: [docs/](docs/)
- 💬 GitHub Issues: Report bugs or request features
- 📧 Community: Join discussions

Happy automating! 🚀
