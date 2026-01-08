# Memex CLI

[![CI](https://github.com/chaorenex1/memex_cli/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/chaorenex1/memex_cli/actions/workflows/ci.yml)
[![Release](https://github.com/chaorenex1/memex_cli/actions/workflows/release.yml/badge.svg)](https://github.com/chaorenex1/memex_cli/actions/workflows/release.yml)

一个面向 **CodeCLI / AI 后端调用** 的"带记忆 + 可回放 + 可恢复"的命令行外壳：

- 把一次运行完整记录为 `run.events.jsonl`（审计、复盘、调试友好）
- 支持 `replay` 重放、`resume` 续跑（基于 `run_id`）
- 内置 **记忆服务命令**：知识检索（`search`）、候选记录（`record-candidate`）、使用反馈（`record-hit`）、会话提取（`record-session`）
- 通过 `config.toml` + 环境变量统一管理 memory/policy/logging/events

## 安装

### 方式 A：下载 Release（推荐）

到 GitHub Releases 下载对应平台的 `memex-cli`（Windows 为 `memex-cli.exe`）。

### 方式 B：从源码构建

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


## 开发与贡献

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```
