# Plugins

Each subdirectory is one plugin, discovered by the runtime because it contains a
`manifest.json`.

```text
plugins/
└── nodara-platform/
    ├── manifest.json
    └── (nodara-platform-plugin.exe when installed)
```

## Executable resolution

The runtime looks for `manifest.executable` in this order:

1. inside the plugin directory itself;
2. next to the running runtime binary (a `cargo build` output tree);
3. under `<binary dir>/plugins/<plugin id>/`.

That means `cargo build` produces a directly runnable development tree: every
plugin binary lands in `target/debug/`, and the manifests here point at it by
name alone.

## Installing a release

For a self-contained deployment, copy each plugin binary next to its manifest:

```powershell
Copy-Item target/release/nodara-platform-plugin.exe plugins/nodara-platform/
Copy-Item target/release/nodara-vision-plugin.exe   plugins/nodara-vision/
```

The core runtime discovers plugins from:

- `<runtime binary dir>/plugins`
- `./plugins`
- every `--plugin-dir` passed to `nodara-cli serve`
