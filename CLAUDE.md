# CLAUDE.md

This file provides guidance for Claude Code when working with this repository.

## Project Overview

Memex-CLI is a Rust-based CLI shell wrapper with memory, replay, and resume capabilities:
- Records execution to `run.events.jsonl` (audit/replay friendly)
- Supports `replay` and `resume` based on run IDs
- Memory retrieval and context injection
- Tool/policy approval gates
- Cross-platform support (Windows, macOS, Linux)

## Workspace Structure

```
memex-cli/
├── core/              # Core execution engine and domain logic (memex-core)
├── plugins/           # Backend, memory, policy, gatekeeper implementations (memex-plugins)
├── cli/               # Binary entry point and TUI (memex-cli)
├── config.toml        # Default configuration
├── .env.online        # Online environment variables
└── .env.offline       # Offline environment variables
```

## Build Commands

```bash
# Build release binary
cargo build -p memex-cli --release

# Size-optimized release
cargo build -p memex-cli --profile size-release

# Development build
cargo build -p memex-cli

# Run tests
cargo test --workspace

# Format code
cargo fmt --all

# Lint with clippy
cargo clippy --workspace --all-targets -- -D warnings
```

## Key Entry Points

- `cli/src/main.rs` - Binary entry point, argument parsing, command dispatch
- `cli/src/app.rs` - Application orchestration, config merging, TUI/standard flow selection
- `cli/src/commands/cli.rs` - CLI argument definitions (RunArgs, ReplayArgs, ResumeArgs)
- `core/src/api.rs` - Public API re-exports
- `core/src/engine/run.rs` - Main execution orchestration
- `plugins/src/factory.rs` - Plugin instantiation (memory, runner, policy, gatekeeper, backend)

## Architecture

### Core Traits (in `core/`)
- `BackendStrategy` - Abstracts codecli/aiservice backends
- `RunnerPlugin` - Process execution
- `PolicyPlugin` - Tool approval logic
- `MemoryPlugin` - Memory operations
- `GatekeeperPlugin` - Quality gates for memory persistence

### Plugin Implementations (in `plugins/`)
- `CodeCliBackendStrategy` / `AiServiceBackendStrategy` - Backend implementations
- `CodeCliRunnerPlugin` / `ReplayRunnerPlugin` - Runner implementations
- `ConfigPolicyPlugin` - Policy rule evaluation
- `MemoryServicePlugin` - Memory API client
- `StandardGatekeeperPlugin` - Quality gate evaluation

### Execution Flow
```
main.rs -> app.rs -> flow_standard.rs or flow_tui.rs
  -> plugins/plan.rs (build_runner_spec)
  -> core/engine/run.rs (run_with_query)
    -> pre.rs (memory search + inject)
    -> run.rs (backend execution)
    -> post.rs (gatekeeper + extract)
```

## Configuration

Config loading priority (highest to lowest):
1. `~/.memex/config.toml`
2. `./config.toml` (current directory)
3. Built-in defaults

Key config sections: `control`, `logging`, `policy`, `memory`, `prompt_inject`, `gatekeeper`, `candidate_extract`, `events_out`, `tui`

## Coding Conventions

- **Formatter:** rustfmt (max_width=100)
- **Linter:** clippy with `-D warnings` (strict)
- **Allowed lints:** `too_many_arguments`, `module_inception`
- **Error handling:** Two-tier errors (`CliError` -> `RunnerError`)
- **Async:** tokio with process, io-util, macros, signal, rt-multi-thread, fs features
- **Testing:** Trait-based design for easy mocking; uses tokio-test, tempfile, mockito

## Dependencies

Key crates: tokio, clap (derive), serde/serde_json, tracing, reqwest, ratatui/crossterm, thiserror, chrono, uuid, toml

## CI/CD

- **ci.yml:** Runs on main/master/develop + PRs; lint job (fmt+clippy), test job (ubuntu + windows)
- **release.yml:** Triggered by `v*` tags; cross-platform builds (macOS ARM/Intel, Linux, Windows)

## Common Tasks

### Adding a new backend
1. Implement `BackendStrategy` trait in `plugins/src/backend/`
2. Add factory function in `plugins/src/factory.rs`
3. Update `BackendKind` enum in `cli/src/commands/cli.rs`

### Adding a new command
1. Add command struct in `cli/src/commands/cli.rs`
2. Add dispatch case in `cli/src/main.rs`
3. Implement handler in `cli/src/app.rs` or new module

### Modifying configuration
1. Update types in `core/src/config/types.rs`
2. Update `config.toml` example
3. Update loading logic in `core/src/config/load.rs` if needed
