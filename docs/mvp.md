```
memex-cli/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── cli.rs                 # clap 参数
│   ├── error.rs               # thiserror 分层
│   ├── runner/
│   │   ├── mod.rs
│   │   ├── codecli.rs         # 启动 codecli
│   │   ├── tee.rs             # stdout/stderr 流式 tee
│   │   └── control.rs         # stdin JSONL 控制通道
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── tool_event.rs      # ToolEvent / Parser
│   │   └── policy_cmd.rs      # policy.decision / abort
│   └── util/
│       └── ring.rs            # Ring buffer
