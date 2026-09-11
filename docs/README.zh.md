# 文档索引

> English: [README.md](README.md)

所有已发布的文档都汇总在这里。

## 语言约定

本目录中的每份指南都有两个版本：

| 文件 | 语言 |
|---|---|
| `name.md` | 英文（默认） |
| `name.zh.md` | 中文 |

两者在结构上保持同步：相同的章节、相同的锚点、相同的示例。修改文档时请同时更新两份。

## 从这里开始

| 文档 | 内容 |
|---|---|
| [project.zh.md](project.zh.md) · [English](project.md) | 项目详细介绍：架构、执行模型、插件、运行时、Studio、Agent、安全、扩展点 |
| [schema.zh.md](schema.zh.md) · [English](schema.md) | 逐个 JSON Schema 文档的逐字段说明，以及工作流 schema 如何产生编辑器内容提示 |
| [nodes.zh.md](nodes.zh.md) · [English](nodes.md) | 节点目录：端口、权限与全部配置项 |

## 快速上手（英文）

| 文档 | 内容 |
|---|---|
| [../README.md](../README.md) | 项目是什么，各部分如何配合 |
| [../QUICKSTART.md](../QUICKSTART.md) | 五分钟端到端演练 |
| [../ARCHITECTURE.md](../ARCHITECTURE.md) | 仓库布局与依赖规则 |
| [../examples/README.md](../examples/README.md) | 可运行的示例工作流 |

## 核心（英文）

| 文档 | 内容 |
|---|---|
| [../RecognizerFramework/docs/architecture.md](../RecognizerFramework/docs/architecture.md) | 各 crate 细节、执行模型、策略与审计 |
| [../RecognizerFramework/docs/node-authoring.md](../RecognizerFramework/docs/node-authoring.md) | 编写能力：进程内或作为插件 |
| [../RecognizerFramework/protocol/plugin-protocol.md](../RecognizerFramework/protocol/plugin-protocol.md) | stdio 上的 JSON-RPC、生命周期、错误码 |
| [../RecognizerFramework/protocol/runtime-api.md](../RecognizerFramework/protocol/runtime-api.md) | HTTP 与 WebSocket API、schema 接口、诊断码 |
| [../RecognizerFramework/schema](../RecognizerFramework/schema) | 生成的 JSON Schema 文档 |
| [../RecognizerFramework/plugins/README.md](../RecognizerFramework/plugins/README.md) | 插件目录结构与可执行文件解析 |

## 客户端（英文）

| 文档 | 内容 |
|---|---|
| [../RecognizerFramework-Studio/README.md](../RecognizerFramework-Studio/README.md) | 可视化编辑器 |
| [../RecognizerFramework-Agent/README.md](../RecognizerFramework-Agent/README.md) | 规划与执行 Agent |

## 项目（英文）

| 文档 | 内容 |
|---|---|
| [../CHANGELOG.md](../CHANGELOG.md) | 版本历史 |
| [../LICENSE](../LICENSE) | MIT 许可证 |

## 重新生成 schema

JSON Schema 文档由 Rust 类型与已安装节点描述符生成。修改类型或描述符后，请重新生成并提交：

```bash
cd RecognizerFramework
cargo run -p rf-cli -- schema --out schema
```

`workflow.schema.json` 由节点目录合成；`--no-capabilities` 生成不含节点目录的版本，
`--plugin-dir` 可纳入插件。CI 会在仓库内 schema 过期时失败，因此不会出现无人察觉的漂移。

节点参考由 `cargo test -p rf-cli` 对照随附节点目录校验，因此
[nodes.zh.md](nodes.zh.md) 与 [nodes.md](nodes.md) 不会与代码脱节。
