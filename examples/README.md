# Examples

This directory contains example workflows demonstrating RecognizerFramework features.

## Basic Examples

### hello-world.json
Simple "Hello World" workflow that logs a message.

```bash
cargo run -p rf-cli -- run examples/hello-world.json
```

### delayed-log.json
Demonstrates System.Delay and multiple log statements.

```bash
cargo run -p rf-cli -- run examples/delayed-log.json
```

### calculation.json
Shows expression evaluation and variable usage.

```bash
cargo run -p rf-cli -- run examples/calculation.json
```

## Windows Automation Examples

### window-find.json
Find windows by title, className, or process name.

```bash
cargo run -p rf-cli -- run examples/windows/window-find.json \
  --allow window.enumerate
```

### input-automation.json
Keyboard and mouse input automation.

```bash
cargo run -p rf-cli -- run examples/windows/input-automation.json \
  --allow input.control
```

### screen-capture.json
Desktop and window screenshot examples.

```bash
cargo run -p rf-cli -- run examples/windows/screen-capture.json \
  --allow desktop.capture
```

## Advanced Examples

### conditional-flow.json
Branching based on calculation results.

### retry-example.json
Demonstrates retry logic and error handling.

### hook-example.json
Before/after hooks for node execution.

## Running Examples

### Using CLI

```bash
# Validate
cargo run -p rf-cli -- validate examples/hello-world.json

# Simulate (dry-run)
cargo run -p rf-cli -- simulate examples/hello-world.json

# Execute
cargo run -p rf-cli -- run examples/hello-world.json
```

### Using Visual Editor

1. Start web server: `python -m http.server 4173`
2. Open: `http://localhost:4173/RecognizerFramework/studio/`
3. Click "Import" and select example file
4. Edit and run in Studio

## Creating Your Own Examples

1. Start with a basic template
2. Add nodes and edges
3. Configure node properties
4. Validate with CLI
5. Test execution

## Example Template

```json
{
  "$schema": "../RecognizerFramework/schema/generated.schema.json",
  "version": 2,
  "nodes": [
    {"id": "start", "kind": "Start", "config": {}},
    {
      "id": "your-node",
      "kind": "System.Log",
      "config": {"message": "Your message"}
    }
  ],
  "edges": [
    {"from": "start", "to": "your-node"},
    {"from": "your-node", "to": "exit"}
  ]
}
```

## Contributing Examples

Found a useful automation? Submit it as an example!

1. Create workflow in `examples/` directory
2. Test thoroughly
3. Add documentation
4. Submit pull request

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.
