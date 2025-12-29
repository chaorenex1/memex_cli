# mem-codecli：设计文档 + 接口定义

## 1. 背景与目标

### 1.1 背景

你当前希望将 `codecli` 作为核心执行引擎（用于开发/生成/工具调用等），并在其前后增加“记忆层”能力：

- **前置检索**：从对话历史/当前输入中提取关键信息，调用记忆服务检索相关 QA 作为上下文注入；
- **后置沉淀**：对本次输出进行质量闸门（Gatekeeper），并将可复用的 Q\&A 以 candidate 形式写回记忆服务；
- **使用命中回传**：当记忆条目被展示/使用时，回传 hit，用于排序优化与质量追踪。

### 1.2 目标

`mem-codecli` 作为一个 Rust 编写的命令行包装器，实现：

1. **与 codecli 兼容**：默认透明转发；可选增强模式启用记忆能力。
2. **流式模式**：支持 **stdin → codecli → stdout** 全链路流式输出，同时可旁路采集日志用于记忆回传。
3. **可审计的“工具接入/审核”机制**：当 codecli/agent 请求接入外部工具（含 MCP server 或内部工具）时，提供可插拔的审批/策略闸门。
4. **可配置**：项目级（project\_id）、记忆服务地址、阈值、注入模板、脱敏策略等通过配置文件/环境变量管理。
5. **工程可维护**：模块化、可测试（含录制回放）、可观测（trace/span/structured log）。

---

## 2. 总体架构

### 2.1 组件划分

- **CLI Frontend（mem-codecli）**
  - 参数解析、配置加载、I/O 模式选择（流式/非流式）、子进程管理。
- **Codecli Runner**
  - 负责启动 `codecli` 子进程，打通 stdin/stdout/stderr（PTY 可选）。
- **Memory Client（HTTP）**
  - 对接 QA 记忆服务（search/candidates/hit/validate/expire 等）。
- **Context Builder**
  - 从当前会话输入/历史文件中抽取 query；
  - 将 search 结果格式化注入到 codecli 的 system/user prompt。
- **Gatekeeper**
  - 对输出进行可复用性评估，决定是否写 candidate；
  - 支持策略：命中不足才写入、强信号才写入、敏感信息过滤等。
- **Tool Access Policy（审核/策略引擎）**
  - 对“接入某工具/某 MCP server”的请求进行 allow/deny/ask；
  - 支持交互式批准或自动策略（基于 allowlist/denylist/标签）。

### 2.2 数据流（高层）

1. 用户输入（stdin/args）→ `mem-codecli`
   2.（可选）记忆检索：`/v1/qa/search` 获取 top-K 相关 QA fileciteturn1file1
2. 构造增强提示词（注入记忆片段）→ 启动 `codecli`
3. `codecli` 流式输出 → `mem-codecli` 透传 stdout（并旁路采集）
   5.（可选）命中回传：`/v1/qa/hit` 记录展示/使用情况 fileciteturn1file0
   6.（可选）候选写入：`/v1/qa/candidates` 写入 candidate QA fileciteturn1file0
   7.（可选）验证闭环：`/v1/qa/validate` 回传验证结果/信号强度 fileciteturn1file3

---

## 3. 命令行设计

### 3.1 子命令概览

- `mem-codecli run [--] <codecli args...>`
  - 运行 codecli（可启用记忆增强）
- `mem-codecli memory search --query "<text>" [--limit N] [--min-score X]`
  - 手工检索记忆
- `mem-codecli memory hit --qa-id ...`
  - 手工回传命中
- `mem-codecli memory candidate --q ... --a ...`
  - 手工写入候选
- `mem-codecli doctor`
  - 检查配置、连通性、鉴权、TLS、超时等
- `mem-codecli policies check --tool <name>`
  - 检查某工具在当前策略下的审批结果

### 3.2 关键参数

- `--project-id <id>`：必需（也可由配置提供）；对应 API payload 的 `project_id` fileciteturn1file1
- `--memory-url <url>`：记忆服务地址
- `--memory-on/--memory-off`：是否启用记忆增强
-（已移除）`--stream`：历史遗留的“透传子进程 stdout/stderr”开关；现在统一由 `--stream-format`（解析方式）+ `--tui`（输出目的地）决定。
- `--inject-mode <system|user|both>`：注入到 system 还是 user
- `--gatekeeper <off|soft|hard>`：
  - off：不写 candidate
  - soft：仅 strong\_signal 才写
  - hard：满足规则即写（更激进）
- `--audit <off|prompt|auto>`：工具接入审核策略
- `--redact <off|basic|strict>`：脱敏强度（过滤 token/密钥/邮箱等）

---

## 4. 记忆增强策略

### 4.1 检索策略（Search）

请求：`POST /v1/qa/search`，payload 包含：

- `project_id`（必填）
- `query`（必填）
- `limit`（默认 6，最大 20）
- `min_score`（默认 0.2） fileciteturn1file1

**Query 构造建议**

- 输入来源：
  - 用户本次输入（stdin/args）
  - 近期对话片段（如果你本地维护 session log）
  - 当前仓库上下文（可选）：README、目录结构摘要、最近修改文件列表
- 拼接策略：
  - 保留关键实体：项目名、工具名、错误栈、文件路径、接口名
  - 限长：例如 800\~1500 字符，避免噪声过大

### 4.2 注入模板（建议）

注入内容建议结构化，以减少模型幻觉与提升可追踪性：

- “检索到的候选知识”区块（按相关性排序）
- 每条包括：`question`、`answer`、`tags`、`confidence`、`qa_id`（若返回包含）

若你们的记忆检索结果包含可追踪锚点（如你此前提到的 `[QA_REF qa-xxxx]`），建议在注入时原样保留，用于后续 hit/validate 的引用一致性。

### 4.3 命中回传（Hit）

请求：`POST /v1/qa/hit`，payload：

- `project_id`（必填）
- `references`：数组元素为 `QAReferencePayload`，至少包含 `qa_id`；并可携带 `shown/used/message_id/context` fileciteturn1file1

**建议**

- shown：被展示给模型/用户即可 true
- used：在最终回答中实际引用/依赖则 true
- message\_id：本次 run 的唯一 ID（mem-codecli 生成 UUID）
- context：可放轻量上下文（如 “injected:system”, “section:design”）

### 4.4 候选写入（Candidate）

请求：`POST /v1/qa/candidates`，payload：

- `project_id`（必填）
- `question`（必填）
- `answer`（必填）
- `summary`（可选）
- `tags`（数组）
- `metadata`（object）
- `source`（可选）
- `author`（可选）
- `confidence`（0\~1，默认 0.5） fileciteturn1file0

**Gatekeeper 规则建议（可配置）**

- 仅当满足以下全部条件才写入：
  1. 本次 search 命中不足（如 top1 < min\_score 或无结果）
  2. 输出可复用：不包含一次性时间/人员私密信息/环境专用值
  3. 结构达标：问题明确、答案步骤化、含边界/注意事项
  4. 安全合规：通过脱敏/秘密扫描

---

## 5. 工具接入审核（Policy & Audit）

你提到“怎么接入 codecli 需要审核之类的请求”，建议将其设计为 mem-codecli 的**通用闸门能力**，不与某一家模型/CLI 强绑定。

### 5.1 触发点

- 当 codecli/agent 输出中出现“请求使用某工具/某 MCP server”的结构化信号时触发。
  - 如果 codecli 支持结构化 tool-call 事件（JSON lines / SSE），优先走结构化解析；
  - 否则用可配置的正则/marker（例如 `TOOL_REQUEST:`）做降级识别。

### 5.2 策略决策

- allow：直接放行
- deny：拒绝并将拒绝原因注入回 codecli 继续推理（让其换方案）
- ask：提示用户交互确认（TUI/stdin）

### 5.3 策略来源

- allowlist/denylist（按 tool name、server\_name、domain、path 前缀等）
- 风险分级（例如：读文件 < 写文件 < 网络访问 < 执行命令）
- 项目模式（dev/test/prod）

### 5.4 审计日志

对每次工具请求记录：

- timestamp、run\_id、project\_id
- tool 标识、参数摘要（脱敏后）
- 决策（allow/deny/ask）与原因
- 结果（成功/失败、耗时）

---

## 6. 配置设计

### 6.1 配置文件（示例）

- 路径：`~/.config/mem-codecli/config.toml`（或项目内 `.mem-codecli.toml`）
- 示例字段：
  - `project_id`
  - `memory.base_url`
  - `memory.timeout_ms`
  - `search.limit`
  - `search.min_score`
  - `inject.mode`
  - `gatekeeper.mode`
  - `audit.mode`
  - `policy.allowlist / denylist`
  - `redact.level`

### 6.2 鉴权与敏感信息

- `MEMORY_API_KEY` 环境变量
- 输出日志默认不落盘敏感字段
- 支持 `--trace-json` 输出结构化 trace（便于 CI/qa-run 接入）

---

## 7. Rust 工程实现建议

### 7.1 crate 选型

- CLI：`clap`
- HTTP：`reqwest`（async）
- async runtime：`tokio`
- 日志：`tracing` + `tracing-subscriber`
- 序列化：`serde` / `serde_json`
- 流式管道：`tokio::process::Command` + `tokio::io::copy_bidirectional`（或手工读写）

### 7.2 进程与流式

目标是：

- stdout：逐行/逐 chunk 透传
- stderr：透传 + 收集诊断
- 同时旁路捕获 stdout/stderr 供 Gatekeeper 分析（可用 tee）

实现策略：

- 使用两个 task：
  - task A：读 child stdout → 写 parent stdout，并写入 ring buffer
  - task B：读 child stderr → 写 parent stderr，并写入 ring buffer
- 结束后对 ring buffer 汇总做 candidate/validate/hit

---

## 8. 记忆服务 API：接口定义（基于 OpenAPI）

### 8.1 Search

- `POST /v1/qa/search` fileciteturn1file8
- Request：`QASearchPayload`（project\_id, query, limit<=20, min\_score） fileciteturn1file1
- Response：规范中 schema 为空（表示服务端返回体未在该片段中约束） fileciteturn1file8

### 8.2 Candidate

- `POST /v1/qa/candidates` fileciteturn1file8
- Request：`QACandidatePayload`（project\_id, question, answer, summary?, tags\[], metadata{}, source?, author?, confidence\[0..1]） fileciteturn1file0

### 8.3 Hit

- `POST /v1/qa/hit` fileciteturn1file8
- Request：`QAHitsPayload`（project\_id, references\[]） fileciteturn1file0
- `QAReferencePayload`：qa\_id 必填；shown/used/message\_id/context 可选 fileciteturn1file1

### 8.4 Validate

- `POST /v1/qa/validate` fileciteturn1file8
- Request：`QAValidationPayload`（project\_id, qa\_id 必填；result/signal\_strength/success/strong\_signal/source/context/client/ts/payload 等） fileciteturn1file4

### 8.5 Expire（可选维护）

- `POST /v1/qa/expire`，payload：`batch_size`（1..1000，默认 200） fileciteturn1file0

---
