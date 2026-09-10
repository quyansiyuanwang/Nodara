# Plugins

Each subdirectory is one plugin, discovered by the runtime because it contains a
`manifest.json`.

```text
plugins/
└── rf-platform/
    ├── manifest.json
    └── (rf-platform-plugin.exe when installed)
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
Copy-Item target/release/rf-platform-plugin.exe plugins/rf-platform/
Copy-Item target/release/rf-vision-plugin.exe   plugins/rf-vision/
```

The core runtime discovers plugins from:

- `<runtime binary dir>/plugins`
- `./plugins`
- every `--plugin-dir` passed to `rf-cli serve`
