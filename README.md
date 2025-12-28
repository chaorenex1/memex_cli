# Memex CLI

[![CI](https://github.com/chaorenex1/memex_cli/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/chaorenex1/memex_cli/actions/workflows/ci.yml)
[![Release](https://github.com/chaorenex1/memex_cli/actions/workflows/release.yml/badge.svg)](https://github.com/chaorenex1/memex_cli/actions/workflows/release.yml)

一个面向 **CodeCLI / AI 后端调用** 的“带记忆 + 可回放 + 可恢复”的命令行外壳：

- 把一次运行完整记录为 `run.events.jsonl`（审计、复盘、调试友好）
- 支持 `replay` 重放、`resume` 续跑（基于 `run_id`）
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

### 方式 C：cargo install（从 Git 仓库）

```bash
cargo install --git https://github.com/chaorenex1/memex_cli.git --package memex-cli
```

## 快速开始

### 1) 准备配置文件（可选但建议）

程序启动时会在“当前工作目录”查找 `config.toml`；不存在则使用内置默认值。

- 示例配置见 `./config.toml`
- 也可以用环境变量覆盖：
  - `MEM_CODECLI_PROJECT_ID`
  - `MEM_CODECLI_MEMORY_URL`
  - `MEM_CODECLI_MEMORY_API_KEY`

### 2) 运行

#### 推荐：使用子命令 `run`

```bash
memex-cli run \
  --backend codex \
  --backend-kind codecli \
  --prompt "帮我总结这个仓库的模块结构，并指出关键入口" \
  --stream
```

后端也可以是 URL（搭配 `--backend-kind aiservice`，或让 `auto` 自动判定）。

#### 兼容模式：不带子命令（透传到本地 codecli）

```bash
memex-cli --codecli-bin codex -- --help
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
  --backend codex \
  --backend-kind codecli \
  --prompt "继续上一轮，给出可执行的下一步" \
  --stream
```

## 配置说明（摘要）

- `logging`：控制 stderr/文件日志与级别
- `policy`：allowlist/denylist（默认拒绝 shell/net，仅允许 fs.read/git.*）
- `memory`：对接 Memory Service（`base_url`/`api_key`/`search_limit`/`min_score`）
- `events_out`：是否输出事件流与输出路径（默认 `./run.events.jsonl`）

更完整的文档见 `docs/`（例如架构与数据流说明）。

## 开发与贡献

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

## 发布（GitHub Actions）

- 推送 tag（例如 `v0.1.0`）会触发 Release 工作流，自动创建 GitHub Release，并构建/上传多平台二进制。
