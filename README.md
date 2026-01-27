# Memex CLI

[![CI](https://github.com/chaorenex1/memex_cli/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/chaorenex1/memex_cli/actions/workflows/ci.yml)
[![Release](https://github.com/chaorenex1/memex_cli/actions/workflows/release.yml/badge.svg)](https://github.com/chaorenex1/memex_cli/actions/workflows/release.yml)

一个面向 **CodeCLI / AI 后端调用** 的"带记忆 + 可回放 + 可恢复"的命令行外壳：

- 把一次运行完整记录为 `run.events.jsonl`（审计、复盘、调试友好）
- 支持 `replay` 重放、`resume` 续跑（基于 `run_id`）
- **本地向量存储**：基于 LanceDB 的本地知识库，支持 Ollama/OpenAI 嵌入
- **内存管理命令**：`search`、`record-candidate`、`record-hit`、`record-session`
- **数据库管理**：`db init`、`db info`、`db export`、`db import`
- **同步功能**：支持本地与远程内存同步（`sync status`、`sync now`）
- 通过 `config.toml` + 环境变量统一管理配置

## 安装

### 一键安装（推荐）

**Linux / macOS (Shell):**
```bash
curl -sSL https://github.com/chaorenex1/memex-cli/releases/latest/download/install_memex.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://github.com/chaorenex1/memex-cli/releases/latest/download/install_memex.ps1 | iex
```

安装完成后，新终端中运行 `memex-cli --help` 验证。

### 手动下载 Release

到 [GitHub Releases](https://github.com/chaorenex1/memex-cli/releases) 下载对应平台的二进制文件。

### 从源码构建

需要 Rust stable。

```bash
cargo build -p memex-cli --release
```

产物位置：

- Windows: `target\\release\\memex-cli.exe`
- macOS/Linux: `target/release/memex-cli`


## 快速开始

### 1) 准备配置文件（可选但建议）

程序启动时会在“当前工作目录”查找 `config.toml`；不存在则使用内置默认值。

- 示例配置见 `./config.toml`
- 可通过环境变量覆盖配置项，详见 `./env.offline` 和 `./env.online`

### 2) 运行

#### 推荐：使用子命令 `run`

```bash
memex-cli run \
  --backend codex \
  --prompt "帮我总结这个仓库的模块结构，并指出关键入口" \
  --stream-format "jsonl"
```

#### json格式输出

codex:

```bash
memex-cli run --backend "codex" --model "deepseek-reasoner" --model-provider "aduib_ai" --prompt "10道四则运算题,写入文件" --stream-format "jsonl"
```

claude:

```bash
memex-cli run --backend "claude" --prompt "10道四则运算题,写入文件" --stream-format "jsonl"
```

gemini:

```bash
memex-cli run --backend "gemini" --prompt "10道四则运算题,写入文件" --stream-format "jsonl"
```

#### text格式输出

codex:

```bash
memex-cli run --backend "codex" --model "deepseek-reasoner" --model-provider "aduib_ai" --prompt "10道四则运算题,写入文件" --stream-format "text"
```

claude:

```bash
memex-cli run --backend "claude" --prompt "10道四则运算题,写入文件" --stream-format "text"
```

gemini:

```bash
memex-cli run --backend "gemini" --prompt "10道四则运算题,写入文件" --stream-format "text"
```

### 项目初始化 (v1.1.0+)

快速初始化项目配置：

```bash
memex-cli init
```

交互式创建 `config.toml` 配置文件，支持：
- 选择内存提供商（本地/远程/混合）
- 配置嵌入服务（Ollama/OpenAI）
- 设置同步选项

### 🆕 结构化文本输入 (v1.0.5+)

Memex-CLI 支持两种输入模式：

#### 普通文本模式 (`--no-structured-text`)

适用于简单的单个提示词：

```bash
# 简单提示
memex-cli run \
  --backend codex \
  --no-structured-text \
  --prompt "编写一个快速排序算法"

# 从文件读取
cat query.txt | memex-cli run \
  --backend claude \
  --no-structured-text \
  --stdin
```

#### 结构化模式（默认）

支持多任务工作流，任务间可定义依赖关系：

```bash
cat > workflow.txt <<'EOF'
---TASK---
id: design-api
backend: claude
workdir: /project
model: claude-sonnet-4
---CONTENT---
设计用户认证 API 接口规范
---END---

---TASK---
id: implement-api
backend: codex
workdir: /project
dependencies: design-api
---CONTENT---
根据设计文档实现 API 代码
---END---

---TASK---
id: write-tests
backend: codex
workdir: /project
dependencies: implement-api
---CONTENT---
编写单元测试和集成测试
---END---
EOF

# 执行完整工作流
memex-cli run --backend codex --stdin < workflow.txt
```

**特性**：
- ✅ 任务依赖管理（自动按拓扑顺序执行）
- ✅ 不同任务使用不同 backend/model
- ✅ 循环依赖检测
- ✅ 文件引用支持
- ✅ 重试和超时配置

**更多示例**：查看 [`examples/`](./examples/) 目录。


### 3) 回放 / 续跑

#### 回放事件

```bash
memex-cli replay --events ./run.events.jsonl --format text
```

#### 续跑（需要 run_id）

```bash
memex-cli resume \
  --run-id <RUN_ID> \
  --backend <backend> \
  --prompt "继续上一轮，给出可执行的下一步" \
  --stream-format "jsonl"
```

```bash
memex-cli resume \
  --run-id <RUN_ID> \
  --backend <backend> \
  --prompt "继续上一轮，给出可执行的下一步" \
  --stream-format "text"
```

### 4) 内存管理命令

Memex CLI 内置了与记忆服务交互的专用命令，用于知识检索、候选记录和使用反馈。

#### 搜索知识库

从记忆服务检索相关知识：

```bash
memex-cli search \
  --query "如何实现 Rust 异步 HTTP 客户端？" \
  --limit 5 \
  --min-score 0.6 \
  --format json
```

参数说明：
- `--query`: 搜索查询（必填）
- `--limit`: 最大返回结果数（默认 5）
- `--min-score`: 最低相关性分数阈值，范围 0.0-1.0（默认 0.6）
- `--format`: 输出格式，可选 `json` 或 `markdown`（默认 json）
- `--project-id`: 项目标识（可选，默认使用当前目录路径）

#### 记录知识候选

将 Q&A 记录到记忆服务：

```bash
memex-cli record-candidate \
  --query "如何配置 Tokio 运行时？" \
  --answer "使用 tokio::runtime::Builder 创建自定义运行时" \
  --tags "rust,tokio,async" \
  --files "src/main.rs,src/runtime.rs" \
  --metadata '{"source":"manual","confidence":0.9}'
```

参数说明：
- `--query`: 问题描述（必填）
- `--answer`: 解决方案（必填）
- `--tags`: 逗号分隔的标签列表（可选）
- `--files`: 逗号分隔的相关文件路径（可选）
- `--metadata`: JSON 格式的额外元数据（可选）
- `--project-id`: 项目标识（可选）

#### 记录知识使用反馈

追踪哪些知识被实际使用：

```bash
memex-cli record-hit \
  --qa-ids "qa-123,qa-456" \
  --shown "qa-123,qa-456,qa-789" \
  --project-id "my-project"
```

参数说明：
- `--qa-ids`: 逗号分隔的已使用知识 ID 列表（必填）
- `--shown`: 逗号分隔的已展示知识 ID 列表（可选，默认等于 qa-ids）
- `--project-id`: 项目标识（可选）

#### 从会话提取并记录知识

从 JSONL 格式的会话记录中提取知识并写入记忆服务：

```bash
# 仅提取不写入
memex-cli record-session \
  --transcript ./run.events.jsonl \
  --session-id "session-20260108" \
  --extract-only

# 提取并写入
memex-cli record-session \
  --transcript ./run.events.jsonl \
  --session-id "session-20260108" \
  --project-id "my-project"
```

参数说明：
- `--transcript`: JSONL 格式的会话记录文件路径（必填）
- `--session-id`: 会话标识符（必填）
- `--project-id`: 项目标识（可选）
- `--extract-only`: 仅提取不写入记忆服务（可选，默认 false）

### 5) 本地数据库管理

Memex CLI 支持基于 LanceDB 的本地向量存储，无需远程服务即可实现知识检索。

#### 初始化本地数据库

```bash
memex-cli db init
```

#### 查看数据库信息

```bash
memex-cli db info --format markdown
```

#### 导出/导入数据

```bash
# 导出为 JSONL
memex-cli db export --output qa_backup.jsonl

# 导出为 CSV
memex-cli db export --output qa_backup.csv --format csv

# 从文件导入
memex-cli db import --input qa_backup.jsonl
```

### 6) 同步功能

支持本地数据库与远程服务之间的数据同步。

```bash
# 查看同步状态
memex-cli sync status

# 立即执行同步
memex-cli sync now

# 查看冲突
memex-cli sync conflicts
```

### 配置本地内存

在 `config.toml` 中配置本地内存：

```toml
[memory]
enabled = true
provider = "local"  # 或 "hybrid" 混合模式

[memory.local]
db_path = "~/.memex/data"
search_limit = 6
min_score = 0.2

[memory.local.embedding]
provider = "ollama"  # 或 "openai"

[memory.local.embedding.ollama]
base_url = "http://localhost:11434"
model = "nomic-embed-text"
dimension = 768

[memory.local.sync]
enabled = true
interval_secs = 300
batch_size = 100
```

### 混合模式（本地 + 远程）

```toml
[memory]
provider = "hybrid"

[memory.hybrid]
sync_strategy = "local-first"  # 或 "remote-first"

[memory.hybrid.remote]
base_url = "https://your-memory-service.com"
api_key = "your-api-key"
timeout_ms = 30000
```


### 远程模式

```toml
[memory]
provider = "service"
enabled = true

# ===== Service Provider (Remote HTTP API) =====
base_url = "https://memory.internal"
api_key = ""
timeout_ms = 10000
search_limit = 6
min_score = 0.2
```


## 架构概览

Memex CLI 采用模块化架构，支持灵活的内存后端：

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI Layer                           │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │
│  │   run    │ │  replay  │ │  resume  │ │  init / db   │  │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └──────┬───────┘  │
│       └────────────┴────────────┴───────────────┘         │
└────────────────────────────────┬────────────────────────────┘
                                 │
┌────────────────────────────────▼────────────────────────────┐
│                      Core Engine                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │
│  │   pre    │ │   run    │ │   post   │ │  gatekeeper  │  │
│  │ (search) │ │(execute) │ │(extract) │ │   (quality)  │  │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └──────┬───────┘  │
│       └────────────┴────────────┴───────────────┘         │
└────────────────────────────────┬────────────────────────────┘
                                 │
┌────────────────────────────────▼────────────────────────────┐
│                    Memory Layer                              │
│  ┌────────────┐  ┌────────────┐  ┌──────────────────────┐  │
│  │   Local    │  │   Hybrid   │  │      Remote         │  │
│  │  (LanceDB) │  │ (Local+Rmt)│  │  (HTTP Service)     │  │
│  └────────────┘  └────────────┘  └──────────────────────┘  │
│                                                              │
│  Embedding:  Ollama  │  OpenAI  │  (Local CPU/GPU removed)  │
└──────────────────────────────────────────────────────────────┘
```

### 内存提供商对比

| 特性 | Local | Hybrid | Remote |
|------|-------|--------|--------|
| 数据存储 | 本地 LanceDB | 本地 + 远程 | 远程 HTTP |
| 网络依赖 | 无 | 可选降级 | 必须 |
| 向量搜索 | ✅ 本地 | ✅ 本地优先 | ❌ 依赖服务 |
| 同步支持 | ❌ | ✅ | N/A |
| 适用场景 | 单机、离线 | 多设备同步 | 团队协作 |

### HTTP 服务器模式

Memex CLI 可作为后台服务运行，支持远程调用：

```bash
# 启动 HTTP 服务器
memex-cli http-server

# 远程模式调用（自动连接到运行中的服务器）
memex-cli run --backend codex --prompt "..." --mode remote
```

配置 `config.toml`：

```toml
[http_server]
mode = "remote"  # 或 "local" 默认本地执行
host = "127.0.0.1"
port = 8001
```


## 开发与贡献

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
