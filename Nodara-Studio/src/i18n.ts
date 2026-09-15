import { Diagnostic, NodeDescriptor, JsonSchema } from "./runtime/types";

export type Locale = "en" | "zh-CN";

const STORAGE_KEY = "nodara.locale";

const EN: Record<string, string> = {
  "app.title": "Nodara Studio",
  "actions.new": "New",
  "actions.import": "Import",
  "actions.export": "Export",
  "actions.undo": "Undo",
  "actions.redo": "Redo",
  "actions.validate": "Validate",
  "actions.run": "Run",
  "actions.pause": "Pause",
  "actions.resume": "Resume",
  "actions.step": "Step",
  "actions.cancel": "Cancel",
  "actions.refresh": "Refresh",
  "actions.open": "Open",
  "actions.applyJson": "Apply JSON",
  "actions.runtimeSchema": "Point $schema at the runtime",
  "actions.approve": "Approve",
  "actions.deny": "Deny",
  "actions.loadPlan": "Load into editor",
  "actions.openRun": "Open run {id}…",
  "actions.addVariable": "Add variable",
  "actions.deleteVariable": "Delete",
  "actions.clearOverride": "Clear override",
  "language.switch": "中文",
  "language.title": "Switch language",
  "toolbar.connecting": "connecting…",
  "toolbar.runtimeUnreachable": "runtime unreachable",
  "toolbar.nodeSummary": "{nodes} node types · {plugins} plugin(s)",
  "toolbar.nodeSummaryFailed": "{nodes} node types · {plugins} plugin(s) · {failed} failed",
  "palette.title": "Nodes",
  "palette.filter": "Filter node types…",
  "palette.emptyRuntime": "No node types reported. Is the runtime running?",
  "palette.emptyFilter": "No node types match that filter.",
  "palette.addHint": "Click to add, or drag onto the canvas.",
  "palette.requires": "Requires: {permissions}",
  "palette.gated": "gated",
  "properties.title": "Properties",
  "inspector.selectNode": "Select a node to edit its configuration.",
  "inspector.nodeMissing": "The selected node no longer exists.",
  "inspector.nodeId": "Node id",
  "inspector.label": "Label",
  "inspector.configuration": "Configuration",
  "inspector.execution": "Execution",
  "inspector.workflow": "Workflow",
  "inspector.workflowId": "Workflow ID",
  "inspector.workflowName": "Name",
  "inspector.workflowDescription": "Description",
  "inspector.workflowTags": "Tags",
  "inspector.workflowTagsHint": "Comma-separated tags.",
  "inspector.variables": "Workflow variables",
  "inspector.variableHint": "Variables are available as {{name}} in node configuration and to runtime expressions.",
  "inspector.variableDescription": "Description",
  "inspector.runOverride": "Run value override",
  "inspector.runOverrideHint": "Used only for the next runs in this Studio session; the workflow default is unchanged.",
  "inspector.secret": "secret",
  "execution.enabled": "Enabled",
  "execution.enabledDetail": "When disabled, the node is skipped and its incoming branches pass through.",
  "execution.condition": "Run condition",
  "execution.conditionDetail": "Optional expression evaluated before this node. A false result skips the node and does not activate its outgoing branches.",
  "execution.conditionPlaceholder": "e.g. retries < 3 && enabled",
  "execution.delayBefore": "Delay before (ms)",
  "execution.delayBeforeDetail": "Wait before invoking this node. Cancellation remains responsive.",
  "execution.delayAfter": "Delay after (ms)",
  "execution.delayAfterDetail": "Wait after this node finishes before activating outgoing branches.",
  "execution.timeout": "Timeout (ms)",
  "execution.timeoutDetail": "Maximum time the plugin may spend on one attempt. Core built-in nodes do not expose a process-level timeout.",
  "execution.continueOnError": "Continue on error",
  "execution.continueOnErrorDetail": "After retries are exhausted, continue through eligible outgoing branches instead of failing the run. An explicit failure branch handles errors without this option. Policy denials and validation errors still stop execution.",
  "execution.retries": "Retries",
  "execution.retriesDetail": "Additional attempts after the first failure. Use 0 to disable retries.",
  "execution.retryDelay": "Retry delay (ms)",
  "execution.retryDelayDetail": "Wait between failed attempts. Cancellation remains responsive.",
  "inspector.noVariables": "No variables declared.",
  "inspector.problems": "Problems",
  "inspector.gated": "Gated capability — the runtime will ask policy before running this node ({permissions}).",
  "form.noConfiguration": "This node has no configuration.",
  "form.resetDefault": "Reset to default",
  "form.invalidJson": "invalid JSON: {message}",
  "canvas.hint": "Click to add at the centre, or drag a palette item to an exact canvas position. Drag from an output port to an input port to connect. Right-click or press Delete to remove the selected item.",
  "canvas.zoomIn": "Zoom in",
  "canvas.zoomOut": "Zoom out",
  "canvas.zoomReset": "Reset zoom to 100%",
  "canvas.fit": "Fit",
  "canvas.autoLayout": "Auto layout",
  "canvas.nodeTitle": "Node: {id}",
  "canvas.connectionTitle": "Connection: {id}",
  "inspector.connectionTitle": "Connection",
  "inspector.edgePath": "{source} → {target}",
  "inspector.edgeLabel": "Label",
  "inspector.edgeLabelDetail": "Optional label shown with the connection.",
  "inspector.edgeCondition": "Condition",
  "inspector.edgeConditionDetail": "Optional expression. The edge is taken only when the result is truthy.",
  "inspector.edgeConditionPlaceholder": "e.g. found && score > 0.8",
  "inspector.edgePorts": "Ports",
  "inspector.edgePortsValue": "{source}.{sourcePort} → {target}.{targetPort}",
  "inspector.edgeBranch": "Outcome",
  "inspector.edgeBranchDetail": "Choose which source-node result can activate this connection. A failure branch handles the error and continues without needing Continue on error.",
  "inspector.edgeBranchAlways": "Always",
  "inspector.edgeBranchSuccess": "Success",
  "inspector.edgeBranchFailure": "Failure",
  "canvas.duplicateNode": "Duplicate node",
  "canvas.disableNode": "Disable node",
  "canvas.enableNode": "Enable node",
  "canvas.nodeDisabled": "disabled",
  "canvas.deleteNode": "Delete node",
  "canvas.deleteConnection": "Delete connection",
  "canvas.selfConnection": "a node cannot connect to itself",
  "canvas.duplicateConnection": "that connection already exists",
  "canvas.singleStart": "only one core.Start node is allowed per workflow",
  "canvas.edgeCondition": "{source} → {target} when {condition}",
  "canvas.edge": "{source} → {target}",
  "validation.notChecked": "not checked",
  "validation.checking": "checking…",
  "validation.valid": "valid",
  "validation.validWarnings": "valid · {warnings} warning(s)",
  "validation.invalid": "{errors} error(s)",
  "validation.invalidWarnings": "{errors} error(s) · {warnings} warning(s)",
  "validation.unavailable": "validation unavailable",
  "validation.waiting": "Waiting for edits to settle before validating.",
  "validation.checkingDetail": "Validating the current workflow automatically.",
  "validation.runtimeUnavailable": "The runtime is unreachable; automatic validation will retry after reconnection.",
  "validation.runBlocked": "run blocked — fix the validation errors shown in Problems",
  "validation.manualValid": "valid — {count} diagnostic(s)",
  "validation.manualInvalid": "invalid — {errors} error(s)",
  "problems.none": "No problems detected.",
  "problems.noStart": "no core.Start node",
  "problems.multipleStart": "workflow declares {count} core.Start nodes; only one is allowed",
  "problems.noEnd": "no core.End node",
  "problems.duplicateNode": "duplicate node id `{id}`",
  "problems.duplicateEdge": "duplicate edge id `{id}`",
  "problems.missingSource": "edge `{id}` has a missing source",
  "problems.missingTarget": "edge `{id}` has a missing target",
  "problems.cycle": "the workflow contains a cycle involving {nodes}",
  "dialog.discardWorkflow": "Discard the current workflow?",
  "status.couldNotImport": "could not import: {message}",
  "status.workflowReplaced": "workflow replaced from JSON",
  "status.invalidWorkflowJson": "invalid workflow JSON: {message}",
  "status.schemaUpdated": "$schema now points at {schema}",
  "status.runAccepted": "run {id} accepted",
  "status.runFailure": "failure [{code}] {message}",
  "status.approval": "{decision} {id}… in session {session}…",
  "status.noPlan": "that session has no plan to load",
  "status.planLoaded": "loaded plan from session {id}…",
  "tabs.events": "Events",
  "tabs.agent": "Agent",
  "tabs.audit": "Audit",
  "tabs.problems": "Problems",
  "tabs.runs": "Runs",
  "tabs.json": "Workflow JSON",
  "audit.currentRun": "this run only",
  "event.studio": "studio",
  "event.runStarted": "run started",
  "event.nodeStarted": "node started",
  "event.progress": "progress",
  "event.nodeFinished": "node finished",
  "event.nodeFailed": "node failed",
  "event.log": "log",
  "event.paused": "paused",
  "event.resumed": "resumed",
  "event.cancelled": "cancelled",
  "event.completed": "completed",
  "event.failed": "failed",
  "event.policy": "policy",
  "event.nodeFinishedBody": "{node} in {duration}ms",
  "event.nodeFailedBody": "{node} [{code}] {message}",
  "event.completedBody": "{nodes} node(s) in {duration}ms",
  "agent.empty": "No agent sessions yet.",
  "agent.tokens": "{provider} · {tokens} token(s)",
  "agent.conversation": "Conversation",
  "agent.approvalRequired": "Approval required",
  "agent.approved": "Approved by {by}",
  "agent.denied": "Denied by {by}",
  "agent.permissions": "node `{node}` ({type}) · permissions: {permissions}",
  "agent.noneDeclared": "none declared",
  "agent.plan": "Plan: {id}",
  "agent.planRejected": "Plan rejected ({errors} error(s))",
  "agent.planSummary": "{nodes} node(s), {edges} edge(s), {warnings} warning(s)",
  "audit.empty": "No audit records yet.",
  "runs.empty": "No runs yet.",
  "runs.id": "Run",
  "runs.workflow": "Workflow",
  "runs.status": "Status",
  "runs.nodes": "Nodes",
  "runs.started": "Started",
  "runs.open": "Open",
  "audit.time": "time",
  "audit.run": "run",
  "audit.category": "category",
  "audit.node": "node",
  "audit.capability": "capability",
  "audit.decision": "decision",
  "audit.message": "message",
  "resizer.palette": "Resize node palette",
  "resizer.properties": "Resize properties panel",
  "resizer.drawer": "Resize bottom panel",
  "resizer.title": "Drag to resize. Double-click to reset.",
  "schema.hint": "$schema: {origin} — {count} node type(s) available to complete",
  "schema.published": "the published schema",
};

const ZH: Record<string, string> = {
  "app.title": "Nodara Studio",
  "actions.new": "新建",
  "actions.import": "导入",
  "actions.export": "导出",
  "actions.undo": "撤销",
  "actions.redo": "重做",
  "actions.validate": "校验",
  "actions.run": "运行",
  "actions.pause": "暂停",
  "actions.resume": "继续",
  "actions.step": "单步",
  "actions.cancel": "取消",
  "actions.refresh": "刷新",
  "actions.open": "打开",
  "actions.applyJson": "应用 JSON",
  "actions.runtimeSchema": "将 $schema 指向运行时",
  "actions.approve": "批准",
  "actions.deny": "拒绝",
  "actions.loadPlan": "载入编辑器",
  "actions.openRun": "打开运行 {id}…",
  "actions.addVariable": "新增变量",
  "actions.deleteVariable": "删除",
  "actions.clearOverride": "清除覆盖",
  "language.switch": "English",
  "language.title": "切换语言",
  "toolbar.connecting": "连接中…",
  "toolbar.runtimeUnreachable": "运行时不可达",
  "toolbar.nodeSummary": "{nodes} 种节点 · {plugins} 个插件",
  "toolbar.nodeSummaryFailed": "{nodes} 种节点 · {plugins} 个插件 · {failed} 个失败",
  "palette.title": "节点",
  "palette.filter": "筛选节点类型…",
  "palette.emptyRuntime": "运行时没有报告任何节点。运行时是否已启动？",
  "palette.emptyFilter": "没有符合筛选条件的节点。",
  "palette.addHint": "单击添加，或拖到画布上。",
  "palette.requires": "需要：{permissions}",
  "palette.gated": "受控",
  "properties.title": "属性",
  "inspector.selectNode": "选择一个节点以编辑其配置。",
  "inspector.nodeMissing": "所选节点已不存在。",
  "inspector.nodeId": "节点 ID",
  "inspector.label": "显示名称",
  "inspector.configuration": "配置",
  "inspector.execution": "执行设置",
  "inspector.workflow": "工作流",
  "inspector.workflowId": "工作流 ID",
  "inspector.workflowName": "名称",
  "inspector.workflowDescription": "描述",
  "inspector.workflowTags": "标签",
  "inspector.workflowTagsHint": "多个标签使用逗号分隔。",
  "inspector.variables": "工作流变量",
  "inspector.variableHint": "节点配置可使用 {{name}} 引用变量，运行时表达式也可以访问。",
  "inspector.variableDescription": "描述",
  "inspector.runOverride": "运行值覆盖",
  "inspector.runOverrideHint": "仅用于当前 Studio 会话后续运行，不修改工作流默认值。",
  "inspector.secret": "敏感值",
  "execution.enabled": "启用",
  "execution.enabledDetail": "禁用后跳过此节点，并让入站分支直接透传。",
  "execution.condition": "运行条件",
  "execution.conditionDetail": "执行此节点前计算的可选表达式；结果为假时跳过节点，并且不激活后续分支。",
  "execution.conditionPlaceholder": "例如：retries < 3 && enabled",
  "execution.delayBefore": "前置延时（毫秒）",
  "execution.delayBeforeDetail": "执行节点前等待，期间仍可响应取消。",
  "execution.delayAfter": "后置延时（毫秒）",
  "execution.delayAfterDetail": "节点完成后等待，再激活后续分支。",
  "execution.timeout": "超时时间（毫秒）",
  "execution.timeoutDetail": "插件单次执行允许的最长时间；核心内置节点不暴露进程级超时。",
  "execution.continueOnError": "失败后继续",
  "execution.continueOnErrorDetail": "重试全部失败后仍继续执行符合条件的后续分支，而不是终止整个运行。明确配置的失败分支可直接处理错误，无需开启此选项。策略拒绝和校验错误仍会终止运行。",
  "execution.retries": "失败重试次数",
  "execution.retriesDetail": "首次失败后的额外执行次数；0 表示不重试。",
  "execution.retryDelay": "重试间隔（毫秒）",
  "execution.retryDelayDetail": "两次失败尝试之间的等待时间，期间仍可响应取消。",
  "inspector.noVariables": "尚未声明变量。",
  "inspector.problems": "问题",
  "inspector.gated": "受控能力 — 运行时会在执行此节点前进行策略审批（{permissions}）。",
  "form.noConfiguration": "此节点没有可配置项。",
  "form.resetDefault": "恢复默认值",
  "form.invalidJson": "JSON 无效：{message}",
  "canvas.hint": "单击节点可添加到画布中央，也可拖到指定位置。从输出端口拖到输入端口即可连线。右键或按 Delete 删除所选项。中键拖动可平移画布，滚轮可缩放。",
  "canvas.zoomIn": "放大",
  "canvas.zoomOut": "缩小",
  "canvas.zoomReset": "恢复 100% 缩放",
  "canvas.fit": "适应",
  "canvas.autoLayout": "自动布局",
  "canvas.nodeTitle": "节点：{id}",
  "canvas.connectionTitle": "连线：{id}",
  "inspector.connectionTitle": "连线属性",
  "inspector.edgePath": "{source} → {target}",
  "inspector.edgeLabel": "标签",
  "inspector.edgeLabelDetail": "显示在连线上的可选标签。",
  "inspector.edgeCondition": "执行条件",
  "inspector.edgeConditionDetail": "可选表达式；仅当结果为真时才会走这条连线。",
  "inspector.edgeConditionPlaceholder": "例如：found && score > 0.8",
  "inspector.edgePorts": "端口",
  "inspector.edgePortsValue": "{source}.{sourcePort} → {target}.{targetPort}",
  "inspector.edgeBranch": "触发结果",
  "inspector.edgeBranchDetail": "选择源节点达到哪种结果时激活此连线。失败分支会自动处理错误并继续，无需开启“失败后继续”。",
  "inspector.edgeBranchAlways": "始终",
  "inspector.edgeBranchSuccess": "成功时",
  "inspector.edgeBranchFailure": "失败时",
  "canvas.duplicateNode": "复制节点",
  "canvas.disableNode": "禁用节点",
  "canvas.enableNode": "启用节点",
  "canvas.nodeDisabled": "已禁用",
  "canvas.deleteNode": "删除节点",
  "canvas.deleteConnection": "删除连线",
  "canvas.selfConnection": "节点不能连接到自身",
  "canvas.duplicateConnection": "该连线已存在",
  "canvas.singleStart": "每个工作流只允许一个 core.Start 节点",
  "canvas.edgeCondition": "{source} → {target}，条件为 {condition}",
  "canvas.edge": "{source} → {target}",
  "validation.notChecked": "未校验",
  "validation.checking": "校验中…",
  "validation.valid": "有效",
  "validation.validWarnings": "有效 · {warnings} 个警告",
  "validation.invalid": "{errors} 个错误",
  "validation.invalidWarnings": "{errors} 个错误 · {warnings} 个警告",
  "validation.unavailable": "无法校验",
  "validation.waiting": "等待编辑停止后自动校验。",
  "validation.checkingDetail": "正在自动校验当前工作流。",
  "validation.runtimeUnavailable": "运行时不可达；恢复连接后会自动重新校验。",
  "validation.runBlocked": "运行已阻止 — 请先修复“问题”中的校验错误",
  "validation.manualValid": "有效 — {count} 条诊断",
  "validation.manualInvalid": "无效 — {errors} 个错误",
  "problems.none": "未发现问题。",
  "problems.noStart": "缺少 core.Start 节点",
  "problems.multipleStart": "工作流声明了 {count} 个 core.Start 节点；只允许一个",
  "problems.noEnd": "缺少 core.End 节点",
  "problems.duplicateNode": "节点 ID `{id}` 重复",
  "problems.duplicateEdge": "连线 ID `{id}` 重复",
  "problems.missingSource": "连线 `{id}` 的源节点不存在",
  "problems.missingTarget": "连线 `{id}` 的目标节点不存在",
  "problems.cycle": "工作流包含循环：{nodes}",
  "dialog.discardWorkflow": "要放弃当前工作流吗？",
  "status.couldNotImport": "导入失败：{message}",
  "status.workflowReplaced": "已从 JSON 替换工作流",
  "status.invalidWorkflowJson": "工作流 JSON 无效：{message}",
  "status.schemaUpdated": "$schema 已指向 {schema}",
  "status.runAccepted": "运行 {id} 已接受",
  "status.runFailure": "失败 [{code}] {message}",
  "status.approval": "已{decision} {id}…，会话 {session}…",
  "status.noPlan": "该会话没有可载入的计划",
  "status.planLoaded": "已载入会话 {id}… 的计划",
  "tabs.events": "事件",
  "tabs.agent": "智能体",
  "tabs.audit": "审计",
  "tabs.problems": "问题",
  "tabs.runs": "运行记录",
  "tabs.json": "工作流 JSON",
  "audit.currentRun": "仅当前运行",
  "event.studio": "工作台",
  "event.runStarted": "运行开始",
  "event.nodeStarted": "节点开始",
  "event.progress": "进度",
  "event.nodeFinished": "节点完成",
  "event.nodeFailed": "节点失败",
  "event.log": "日志",
  "event.paused": "已暂停",
  "event.resumed": "已继续",
  "event.cancelled": "已取消",
  "event.completed": "已完成",
  "event.failed": "已失败",
  "event.policy": "策略",
  "event.nodeFinishedBody": "{node} 用时 {duration}ms",
  "event.nodeFailedBody": "{node} [{code}] {message}",
  "event.completedBody": "{nodes} 个节点，用时 {duration}ms",
  "agent.empty": "尚无智能体会话。",
  "agent.tokens": "{provider} · {tokens} tokens",
  "agent.conversation": "对话",
  "agent.approvalRequired": "需要审批",
  "agent.approved": "{by} 已批准",
  "agent.denied": "{by} 已拒绝",
  "agent.permissions": "节点 `{node}`（{type}）· 权限：{permissions}",
  "agent.noneDeclared": "未声明",
  "agent.plan": "计划：{id}",
  "agent.planRejected": "计划被拒绝（{errors} 个错误）",
  "agent.planSummary": "{nodes} 个节点，{edges} 条连线，{warnings} 个警告",
  "audit.empty": "尚无审计记录。",
  "runs.empty": "尚无运行记录。",
  "runs.id": "运行",
  "runs.workflow": "工作流",
  "runs.status": "状态",
  "runs.nodes": "节点数",
  "runs.started": "开始时间",
  "runs.open": "打开",
  "audit.time": "时间",
  "audit.run": "运行",
  "audit.category": "类别",
  "audit.node": "节点",
  "audit.capability": "能力",
  "audit.decision": "决策",
  "audit.message": "消息",
  "resizer.palette": "调整节点面板宽度",
  "resizer.properties": "调整属性面板宽度",
  "resizer.drawer": "调整底部面板高度",
  "resizer.title": "拖动调整大小，双击恢复默认值。",
  "schema.hint": "$schema：{origin} — 可使用 {count} 种节点完成",
  "schema.published": "已发布 Schema",
};

const NODE_ZH: Record<string, { display_name: string; category: string; description: string }> = {
  "core.Calculate": { display_name: "计算", category: "核心", description: "计算数值表达式" },
  "core.End": { display_name: "结束", category: "核心", description: "终止工作流" },
  "core.Log": { display_name: "日志", category: "核心", description: "向运行日志写入消息" },
  "core.SetVariable": { display_name: "设置变量", category: "核心", description: "向运行作用域发布值" },
  "core.Start": { display_name: "开始", category: "核心", description: "工作流入口点" },
  "system.Clipboard": { display_name: "剪贴板", category: "系统", description: "读取或替换剪贴板文本" },
  "system.Delay": { display_name: "延时", category: "系统", description: "等待固定时长" },
  "vision.Ocr": { display_name: "OCR", category: "视觉", description: "使用已配置后端从图像中提取文本" },
  "vision.TemplateMatch": { display_name: "模板匹配", category: "视觉", description: "在截图帧中定位模板图像" },
  "windows.Desktop.Capture": { display_name: "桌面截图", category: "桌面", description: "截取整个主显示器或指定区域" },
  "windows.Input.Keyboard": { display_name: "键盘", category: "输入", description: "向当前窗口发送按键或组合键" },
  "windows.Input.Mouse": { display_name: "鼠标", category: "输入", description: "移动鼠标并模拟按键" },
  "windows.Input.Text": { display_name: "文本输入", category: "输入", description: "向当前窗口输入文本" },
  "windows.Window.Capture": { display_name: "窗口截图", category: "窗口", description: "截取窗口及边框" },
  "windows.Window.Find": { display_name: "查找窗口", category: "窗口", description: "按标题或类名查找窗口" },
  "windows.Window.Focus": { display_name: "聚焦窗口", category: "窗口", description: "将窗口置于前台" },
};

const SCHEMA_ZH: Record<string, { title: string; description?: string }> = {
  "Expression": { title: "表达式", description: "针对运行作用域计算的算术或比较表达式，支持 `+`、`-`、`*`、`/`、`%`、`^`、比较和 `&&`/`||`。" },
  "Output variable": { title: "输出变量", description: "用于发布结果的变量名。" },
  "Exit code": { title: "退出码", description: "工作流结束时运行时报告的进程退出码。" },
  "Level": { title: "日志级别", description: "日志严重级别，默认 `info`。" },
  "Message": { title: "消息", description: "支持 `{{variable}}` 插值的消息模板。" },
  "Variable name": { title: "变量名", description: "在运行作用域中发布该值使用的名称。" },
  "Value": { title: "值", description: "要保存的值。字符串支持 `{{variable}}` 插值，可接受任意 JSON 值。" },
  "Action": { title: "操作", description: "要执行的节点操作。" },
  "Text": { title: "文本", description: "要处理的文本，支持 `{{variable}}` 插值。" },
  "Duration (ms)": { title: "持续时间（毫秒）", description: "等待时长，等待期间会响应取消。" },
  "Image": { title: "图像", description: "截图节点生成的制品 ID，或要读取的图像路径。" },
  "Language": { title: "语言", description: "传给 OCR 后端的 BCP-47 语言标签。" },
  "Frame": { title: "帧", description: "截图节点生成的制品 ID，或要搜索的图像路径。" },
  "Template": { title: "模板", description: "要查找的模板制品 ID 或图像路径。" },
  "Threshold": { title: "匹配阈值", description: "视为匹配所需的最低归一化互相关分数。" },
  "Height": { title: "高度", description: "区域高度（像素）。" },
  "Width": { title: "宽度", description: "区域宽度（像素）。" },
  "X": { title: "X 坐标", description: "屏幕像素 X 坐标。" },
  "Y": { title: "Y 坐标", description: "屏幕像素 Y 坐标。" },
  "Key chord": { title: "按键/组合键", description: "向当前窗口发送的按键；修饰键和普通键使用 `+` 连接。" },
  "Key interval (ms)": { title: "按键间隔（毫秒）", description: "连续按键之间的延迟；0 表示尽可能快。" },
  "Window class": { title: "窗口类名", description: "要匹配的 Win32 窗口类名，例如 `Notepad`。" },
  "Exact match": { title: "精确匹配", description: "要求标题和类名完全匹配，而不是包含匹配。" },
  "Foreground window": { title: "使用前台窗口", description: "直接使用当前前台窗口，优先于标题和类名条件。" },
  "Window title": { title: "窗口标题", description: "要匹配的窗口标题；默认使用包含匹配。" },
  "Focus target window": { title: "聚焦目标窗口", description: "发送输入前查找并聚焦目标窗口；关闭时向当前前台窗口发送输入。" },
};

const STATUS_ZH: Record<string, string> = {
  draft: "草稿",
  planning: "规划中",
  awaiting_approval: "等待审批",
  ready: "计划就绪",
  running: "运行中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

const CATEGORY_EN: Record<string, string> = {
  run_started: "run started",
  run_finished: "run finished",
  node_started: "node started",
  node_finished: "node finished",
  node_failed: "node failed",
  capability_evaluated: "capability",
  approval: "approval",
  log: "log",
};

const CATEGORY_ZH: Record<string, string> = {
  run_started: "运行开始",
  run_finished: "运行结束",
  node_started: "节点开始",
  node_finished: "节点完成",
  node_failed: "节点失败",
  capability_evaluated: "能力评估",
  approval: "审批",
  log: "日志",
};

function detectLocale(): Locale {
  try {
    const stored = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (stored === "en" || stored === "zh-CN") return stored;
    return globalThis.navigator?.language?.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
  } catch {
    return "en";
  }
}

let activeLocale: Locale = detectLocale();

export function getLocale(): Locale {
  return activeLocale;
}

export function setLocale(locale: Locale): void {
  activeLocale = locale;
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, locale);
  } catch {
    // Storage can be unavailable in hardened webviews; the current session still works.
  }
  document.documentElement.lang = locale;
}

export function toggleLocale(): Locale {
  const next: Locale = activeLocale === "en" ? "zh-CN" : "en";
  setLocale(next);
  return next;
}

export function t(key: string, vars: Record<string, unknown> = {}): string {
  const catalog = activeLocale === "zh-CN" ? ZH : EN;
  const template = catalog[key] ?? EN[key] ?? key;
  return template.replace(/\{(\w+)\}/g, (_, name: string) => String(vars[name] ?? `{${name}}`));
}

export function isChinese(): boolean {
  return activeLocale === "zh-CN";
}

export function applyStaticTranslations(root: ParentNode = document): void {
  document.documentElement.lang = activeLocale;
  for (const node of root.querySelectorAll<HTMLElement>("[data-i18n]")) {
    node.textContent = t(node.dataset.i18n!);
  }
  for (const node of root.querySelectorAll<HTMLElement>("[data-i18n-placeholder]")) {
    node.setAttribute("placeholder", t(node.dataset.i18nPlaceholder!));
  }
  for (const node of root.querySelectorAll<HTMLElement>("[data-i18n-title]")) {
    const title = t(node.dataset.i18nTitle!);
    node.setAttribute("title", title);
  }
  for (const node of root.querySelectorAll<HTMLElement>("[data-i18n-aria-label]")) {
    node.setAttribute("aria-label", t(node.dataset.i18nAriaLabel!));
  }
  const language = document.getElementById("btn-language");
  if (language) language.textContent = t("language.switch");
}

function localizeSchema(schema: JsonSchema): JsonSchema {
  if (activeLocale !== "zh-CN") return schema;
  const output: JsonSchema = { ...schema };
  const translated = schema.title ? SCHEMA_ZH[schema.title] : undefined;
  if (translated) {
    output.title = translated.title;
    if (translated.description) output.description = translated.description;
  }
  if (schema.properties) {
    output.properties = Object.fromEntries(
      Object.entries(schema.properties).map(([key, value]) => [key, localizeSchema(value)]),
    );
  }
  if (schema.items) output.items = localizeSchema(schema.items);
  return output;
}

export function localizeDescriptor(descriptor: NodeDescriptor): NodeDescriptor {
  if (activeLocale !== "zh-CN") return descriptor;
  const translation = NODE_ZH[descriptor.node_type];
  return {
    ...descriptor,
    display_name: translation?.display_name ?? descriptor.display_name,
    category: translation?.category ?? descriptor.category,
    description: translation?.description ?? descriptor.description,
    config_schema: localizeSchema(descriptor.config_schema),
  };
}

export function localizeDiagnostic(diagnostic: Diagnostic): Diagnostic {
  if (activeLocale !== "zh-CN") return diagnostic;
  const node = diagnostic.node_id ? `\`${diagnostic.node_id}\`` : "节点";
  const messages: Record<string, string> = {
    WF100: "工作流 schema_version 不受支持",
    WF101: "工作流 ID 不能为空",
    WF102: "节点 ID 不能为空",
    WF103: `节点 ID ${node} 重复`,
    WF104: `节点 ${node} 的类型不能为空`,
    WF110: "连线 ID 不能为空",
    WF111: "连线 ID 重复",
    WF112: "连线源节点不存在",
    WF113: "连线目标节点不存在",
    WF114: "连线不能指向自身",
    WF120: "工作流缺少 core.Start 节点",
    WF121: "工作流包含多个 core.Start 节点；只允许一个",
    WF122: "工作流缺少 core.End 节点",
    WF130: "工作流包含循环",
    WF131: `${node} 无法从入口节点到达`,
    WF132: `${node} 没有后续连线`,
    WF140: `未知节点类型 \`${diagnostic.message.match(/`([^`]+)`/)?.[1] ?? ""}\``,
    WF150: `${node} 配置引用了未声明的变量`,
    WF151: `${node} 运行条件引用了未声明的变量`,
    WF152: `连线 ${diagnostic.edge_id ? `\`${diagnostic.edge_id}\`` : ""} 的执行条件引用了未声明的变量`,
  };
  return {
    ...diagnostic,
    message: messages[diagnostic.code] ?? diagnostic.message,
  };
}

export function localizeProblem(problem: string): string {
  if (activeLocale !== "zh-CN") return problem;
  let match = problem.match(/^workflow declares (\d+) core\.Start nodes; only one is allowed$/);
  if (match) return t("problems.multipleStart", { count: Number(match[1]) });
  match = problem.match(/^duplicate node id `([^`]+)`$/);
  if (match) return t("problems.duplicateNode", { id: match[1] });
  match = problem.match(/^duplicate edge id `([^`]+)`$/);
  if (match) return t("problems.duplicateEdge", { id: match[1] });
  match = problem.match(/^edge `([^`]+)` has a missing source$/);
  if (match) return t("problems.missingSource", { id: match[1] });
  match = problem.match(/^edge `([^`]+)` has a missing target$/);
  if (match) return t("problems.missingTarget", { id: match[1] });
  match = problem.match(/^the workflow contains a cycle involving (.+)$/);
  if (match) return t("problems.cycle", { nodes: match[1] });
  if (problem === "no core.Start node") return t("problems.noStart");
  if (problem === "no core.End node") return t("problems.noEnd");
  return problem;
}

const RUN_STATUS_ZH: Record<string, string> = {
  pending: "等待中",
  running: "运行中",
  paused: "已暂停",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

export function localizeRunStatus(status: string): string {
  return activeLocale === "zh-CN" ? (RUN_STATUS_ZH[status] ?? status) : status;
}

export function localizeAgentStatus(status: string): string {
  return activeLocale === "zh-CN" ? (STATUS_ZH[status] ?? status) : status.replace(/_/g, " ");
}

export function localizeAuditCategory(category: string): string {
  return activeLocale === "zh-CN"
    ? (CATEGORY_ZH[category] ?? category)
    : (CATEGORY_EN[category] ?? category.replace(/_/g, " "));
}
