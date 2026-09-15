# 项目详细介绍

> English: [project.md](project.md)

Nodara 是一套面向 Windows 桌面的工作流自动化框架，以及围绕它的整套生态：
一个无界面运行时（headless runtime）、一个可视化编辑器、一个自主 Agent，以及一套让第三方
在不修改任何既有代码的前提下扩展能力的插件模型。

工作流是**文档**，不是程序。文档里声明能力（节点），把节点连成有向图，然后把图交给运行时。
运行时负责校验图、依据策略决定每个副作用是否放行、按照依赖顺序执行节点，并把每一步都发布为事件。

## 1. 项目解决什么问题

| 问题 | Nodara 的答案 |
|---|---|
| 桌面任务太不规则，录屏式宏搞不定 | 带条件、变量和错误分支的节点图 |
| 让非程序员也能使用自动化 | Studio 依据节点描述符动态渲染节点面板与配置表单，UI 里不硬编码任何节点类型 |
| 让模型安全地操作桌面 | Agent 只负责规划；每一次能力调用都必须经过运行时的策略、审批与审计层 |
| 不 fork 核心就扩展能力 | 能力即插件：运行时自动发现，编辑器与 Agent 自动获得，无需重新编译 |
| 客户端与核心不脱节 | 一套 JSON Schema 契约，由 Rust 类型生成，并由运行时对外提供 |

## 2. 仓库结构

| 路径 | 作用 |
|---|---|
| `Nodara-Core/` | 核心：`nodara-schema` 契约、`nodara-core` 引擎与 SDK、`nodara-plugin` 宿主、`nodara-runtime` 服务、官方能力集 `nodara-platform`/`nodara-vision`、`nodara-cli`、`nodara-testkit` |
| `Nodara-Studio/` | 可视化编辑器：Vite + TypeScript Web 应用与 Tauri v2 桌面外壳 |
| `Nodara-Agent/` | 规划与执行 Agent：自然语言生成工作流、监督运行、解释结果、读取审计 |
| `examples/` | 可直接运行的工作流文档（`hello-world`、`branching`、`delayed-log`、`window-find`，以及一个旧版示例） |
| `docs/` | 本指南、[Schema 详解](schema.zh.md)、[节点参考](nodes.zh.md) 与文档索引 |

除此之外的一切（插件、业务工作流）都是数据。一个部署 = 运行时二进制 + 一个插件目录 + 你连接的客户端。

## 3. 架构

```text
        Nodara-Studio        Nodara-Agent
                    \                              /
                     \   HTTP + WebSocket + JSON  /
                      \                        /
                       v                      v
                  +--------------------------------+
                  |   Nodara-Core Runtime  |   nodara-runtime
                  +--------------------------------+
                              |
                  +-----------+-----------+
                  |                       |
            nodara-core (engine)        nodara-plugin (host)
                  |                       |
                  +----------+------------+
                             |
                        nodara-schema  (contracts)
```

三条规则由 crate 依赖图强制保证，而不是靠约定：

1. **依赖方向只有指向核心这一种。** Studio 与 Agent 都是普通 API 客户端，不链接任何核心内部实现。
2. **Studio 与 Agent 互不感知。** 若需协作，一律通过运行时的会话、运行与事件进行。
3. **能力是插件，不是核心模块。** `nodara-core` 里不出现 `nodara-platform`/`nodara-vision`；运行时发现官方能力集的
   方式与发现第三方插件完全一致。

Agent 的隔离是最极端的例子：它只依赖 `nodara-schema`，不依赖核心的其它任何东西，因此它**在物理上**
无法绕过运行时的策略层去执行能力。"权限不取决于提示词"是这个依赖图的直接性质。

### Crate 一览

| Crate | 职责 | 依赖 |
|---|---|---|
| `nodara-schema` | 工作流、插件 manifest、节点描述符、执行事件、会话、工具调用契约；校验；图算法；迁移；JSON Schema 生成 | `serde`、`schemars` |
| `nodara-core` | `NodeExecutor` SDK、`CapabilityRegistry`、`ExtensionRegistry`、`WorkflowEngine`、运行控制、策略、审计、事件、内置节点、表达式求值 | `nodara-schema` |
| `nodara-plugin` | stdio 上的 JSON-RPC 2.0、进程内传输、插件发现、插件宿主 | `nodara-schema`、`nodara-core` |
| `nodara-runtime` | 组装根：引擎 + 插件 + 策略 + 审计、运行管理、Agent 会话、HTTP/WebSocket API | `nodara-schema`、`nodara-core`、`nodara-plugin` |
| `nodara-platform` | Windows 键鼠输入、窗口管理、屏幕捕获、剪贴板和进程执行 | `nodara-core`、`nodara-schema` |
| `nodara-vision` | 模板匹配、可插拔 OCR | `nodara-core`、`nodara-schema` |
| `nodara-cli` | `validate`、`run`、`simulate`、`inspect`、`migrate`、`plugins`、`schema`、`serve` | 以上全部 |
| `nodara-testkit` | 工作流构造器、记录型执行器、进程内插件测试夹具 | `nodara-schema`、`nodara-core`、`nodara-plugin` |

## 4. 执行模型

```text
RunManager.start
   |
   +--> WorkflowEngine.run          （每次运行独占一个操作系统线程）
          |
          +--> validate             （感知能力注册表，且不会在第一个错误处停下）
          +--> 拓扑排序 + 分支激活
          +--> 对每个被激活的节点：
          |      control.await_permission()   <- 暂停 / 单步 / 取消
          |      policy.decide()              -> 事件 + 审计记录
          |      executor.execute()           -> 本地调用或插件往返
          |      发布 NodeFinished，激活带条件的边
          |
          +--> RunCompleted | RunFailed | RunCancelled
```

* **节点是同步的。** `NodeExecutor::execute` 返回 `Result`；节点可以阻塞在 Win32 调用、插件往返或
  睡眠上而不拖垮异步运行时，因为每次运行都有自己的线程。SDK 中不存在 `async` 传染。
* **暂停是确定性的。** 运行控制在节点边界检查，因此被暂停的运行永远停在两个节点之间。取消更强：
  `RunControl` 对执行器可见，长时间运行的节点会轮询它，所以即使在节点内部取消也能及时生效。
* **分支是数据驱动的。** 没有 `condition` 的边永远走；带条件的边在表达式为真时走。未被激活的节点
  永远不会执行。
* **变量是数据通道。** 节点把值写入运行作用域（`SetVariable`、`output_var`、输出端口），模板通过
  `{{name}}` 读取；图像等大对象以 artifact id 传递。
* **失败是结构化的。** 节点失败携带稳定的错误码与消息；引擎以 `RunFailed` 结束运行，除非该失败属于
  "可由重新规划修复"（`E_INVALID_CONFIG`、`E_EXECUTION`），此时 Agent 会话可以再规划一轮。

## 5. 能力与插件模型

能力就是任何实现 `NodeExecutor` 的类型。它通过
[`NodeDescriptor`](schema.zh.md#5-节点描述符) 描述自己：节点类型、显示名、分类、描述、端口、
配置项的 JSON Schema、所需权限，以及是否为危险操作。

同一份实现有两种运行方式：

* **进程内** —— 注册进运行时的 `CapabilityRegistry`，并由
  `ExtensionRegistry` 统一记录来源、类型、节点和加载状态；
* **插件** —— 用 `nodara_plugin::serve_stdio` 通过 stdio 提供服务，并在二进制旁放置 `manifest.json`。manifest 还可通过 `features` 声明非节点功能，并加入同一个 `ExtensionRegistry`。

运行时通过 JSON-RPC 懒加载插件（`initialize`、`describe`、`execute`、`cancel`、`health`、`shutdown`），
并把协议层故障降级为普通节点失败：崩溃、超时或断连只会让该节点失败，不会拖垮运行时。仅描述符注册
永远不会覆盖已经可运行的节点类型，所以一个"装了一半"的插件目录不会遮蔽正常能力。

细节见 [插件协议（英文）](../Nodara-Core/protocol/plugin-protocol.md) 与
[编写节点（英文）](../Nodara-Core/docs/node-authoring.md)。

## 6. 运行时 API

运行时是唯一真正执行东西的进程，两个客户端都很薄：

| 方法 | 路径 | 作用 |
|---|---|---|
| `GET` | `/api/v1` | 服务标识、三个版本轴、已发布的 schema 名称 |
| `GET` | `/api/v1/health` | 存活状态与能力数量 |
| `GET` | `/api/v1/plugins` | 已安装插件与加载失败信息 |
| `GET` | `/api/v1/extensions` | 统一列出内置、进程内与插件扩展注册 |
| `GET` | `/api/v1/node-types` | 每个节点类型的描述符 |
| `GET` | `/api/v1/schema/{document}` | 按当前部署合成的 JSON Schema |
| `POST` | `/api/v1/workflows/validate` | 返回文档的诊断信息 |
| `POST` | `/api/v1/runs` | 启动一次运行（可绑定 Agent 会话） |
| `GET` | `/api/v1/runs[/{id}]` | 列出或查看运行 |
| `POST` | `/api/v1/runs/{id}/{pause,resume,step,cancel}` | 控制运行 |
| `WS` | `/api/v1/runs/{id}/events` | 回放并流式推送事件 |
| `GET` | `/api/v1/runs/{id}/event-log` | 以 REST 获取同一事件序列 |
| `GET`/`POST` | `/api/v1/agent/sessions…` | 会话、计划、消息、审批 |
| `GET` | `/api/v1/audit` | 运行时放行、拒绝与记录了什么 |

完整参考：[Runtime API（英文）](../Nodara-Core/protocol/runtime-api.md)。

## 7. Studio

Studio 是一个"由描述符驱动"的编辑器。启动时它拉取 `/api/v1/node-types`，并据此构建节点面板、
配置表单与能力徽标；唯一硬编码的只有新建文档时的 `core.Start` 与 `core.End`。安装一个插件即可改变
UI，无需重新构建前端。
 底部页签由 Studio `FeatureRegistry` 统一组装，内置面板与未来的宿主/插件
UI 贡献共享同一套有序注册路径。

它提供画布、由描述符配置 schema 生成的属性面板、带事件流与控制按钮（`pause`/`resume`/`step`/`cancel`）
的运行视图、带计划预览与审批提示的 Agent 面板、审计视图，以及原始工作流 JSON 视图。JSON 视图会保留
文档的 `$schema` 引用，并支持一键指向正在运行的运行时 —— 这正是任何支持 JSON Schema 的编辑器能给出
补全与文档提示的原因。

## 8. Agent

Agent 把目标变成工作流，然后操作它 —— 且始终通过运行时 API：

| 命令 | 作用 |
|---|---|
| `capabilities` | 列出运行时提供的节点类型及其权限 |
| `plan "<目标>"` | 生成工作流文档（可用 `--from` 基于现有文档做修改） |
| `run "<目标>"` | 规划、启动运行、观察事件，并在可修复的失败上重新规划 |
| `explain` | 用自然语言解释工作流、运行结果或节点失败 |
| `sessions` / `approve` | 查看会话与待审批项，授予或拒绝 |
| `control <run> <action>` | 暂停、恢复、单步或取消运行 |
| `audit` | 读取运行时审计日志 |
| `replay <trace>` | 回放记录下来的决策轨迹 |

护栏：`--safe` 拒绝一切带副作用的节点，`--allow <节点类型>` 把 Agent 限制在显式白名单内；
重新规划是选择性的 —— 只有模型能通过改写解决的失败（`E_INVALID_CONFIG`、`E_EXECUTION`）才会触发下一轮。

## 9. 安全、审批与审计

执行从不依赖客户端"守规矩"：

* 每个描述符都声明 `permissions` 与 `dangerous`；运行时在每次调用前向策略请求裁决。
* 策略：`DefaultPolicy`（放行安全节点，特权节点一律要求审批）、`AllowlistPolicy`（白名单外全拒，
  可按权限键控）、`AllowAllPolicy`，以及 `PolicyChain`（拒绝优先，其次审批，最后放行）。
* 开启审批后，运行时会针对所属 Agent 会话发起审批请求，并**阻塞运行线程**直到操作员答复。
  超时等于拒绝；未绑定会话的运行会被直接拒绝。
* 当前官方能力集中的特权权限：`input.control`（键盘、鼠标、文本）、`window.control`（窗口置前）、
  `screen.capture`、`clipboard`、`process.execute`（外部命令）、`vision.analyze`。
* 审计日志记录每一次策略裁决、节点结果与日志；可用 `GET /api/v1/audit` 或 `nodara-agent audit` 读取。

## 10. 版本与迁移

| 契约 | 字段 | 当前值 |
|---|---|---|
| 工作流文档 | `schema_version` | `2.0` |
| 插件/运行时线协议 | `protocol_version` | `1` |
| 公开 HTTP API | `api_version` | `v1` |

主版本号变化表示破坏性变更；次版本变化向前兼容：未知字段会在往返中保留，未知节点类型由"感知能力的
校验"而不是解析器报错。`nodara-cli migrate` 可升级旧文档（节点类型命名空间化、`from`/`to` 改为
`source`/`target`、`seconds` 改为 `duration_ms`、`text` 改为 `message`），并保留文档里的 `$schema` 引用。

## 11. 构建、运行、测试

```bash
# 核心：校验、运行、启动服务
cd Nodara-Core
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p nodara-cli -- validate ../examples/hello-world.json
cargo run -p nodara-cli -- run ../examples/hello-world.json --in-process
cargo run -p nodara-cli -- serve --in-process --port 8710

# 编辑器
cd ../Nodara-Studio
npm ci && npm test && npm run build

# Agent
cd ../Nodara-Agent
cargo test --workspace
NODARA_LLM_API_KEY=sk-... cargo run -p nodara-agent -- plan "打开记事本并输入 hello"
```

## 12. 扩展点

| 我想… | 做法 |
|---|---|
| 增加一个能力 | 实现 `NodeExecutor`，进程内注册或作为插件发布；见[编写节点（英文）](../Nodara-Core/docs/node-authoring.md) |
| 让能力的表单与提示更好用 | 为每个配置项写清 title、description、default、enum、examples；见 [Schema 详解](schema.zh.md#10-编写对用户友好的配置-schema) |
| 改变安全规则 | 实现 `CapabilityPolicy`，或通过 CLI/API 配置 `AllowlistPolicy` |
| 替换审批通道 | 实现 `nodara_core::ApprovalHandler` |
| 增加一个客户端 | 对接 `api_version v1`，核心无需任何改动 |
| 增加插件传输方式 | 在 `nodara-plugin` 中实现传输层；描述符契约保持不变 |

## 13. 接下来读什么

| 主题 | 文档 |
|---|---|
| 每个 JSON Schema 文档、逐字段说明 | [schema.zh.md](schema.zh.md) |
| 节点目录：端口、权限、配置 | [nodes.zh.md](nodes.zh.md) |
| 仓库布局与依赖规则（英文） | [ARCHITECTURE.md](../ARCHITECTURE.md) |
| 五分钟上手（英文） | [QUICKSTART.md](../QUICKSTART.md) |
| Crate 细节、策略路径、执行模型（英文） | [Nodara-Core/docs/architecture.md](../Nodara-Core/docs/architecture.md) |
| 插件线协议（英文） | [Nodara-Core/protocol/plugin-protocol.md](../Nodara-Core/protocol/plugin-protocol.md) |
| 运行时 API（英文） | [Nodara-Core/protocol/runtime-api.md](../Nodara-Core/protocol/runtime-api.md) |
| 版本历史（英文） | [CHANGELOG.md](../CHANGELOG.md) |
