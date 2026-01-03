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


## 开发与贡献

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```
