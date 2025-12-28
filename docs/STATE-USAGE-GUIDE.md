# State Management Usage Guide

This guide covers:
- AppContext usage (developer integration)
- End-user CLI usage (enable state management and logs)

## Scope

Project: memex-cli  
Components: `AppContext`, `StateManager`, runtime event stream

---

## Developer Guide: AppContext

### What AppContext Provides

`AppContext` centralizes core runtime dependencies:
- `AppConfig`
- `StateManager` (optional, `Arc`)
- `EventsOutTx` (optional)

Location:
- `core/src/context.rs`

### Creating AppContext

Create it in CLI (or any app entry) and pass it down.

```rust
use memex_core::context::AppContext;
use memex_core::state::StateManager;
use std::sync::Arc;

let cfg = memex_core::config::load_default()?;
let state_manager = Some(Arc::new(StateManager::new()));
let ctx = AppContext::new(cfg, state_manager).await?;
```

### Accessing Components

```rust
let cfg = ctx.cfg();
let manager = ctx.state_manager(); // Option<Arc<StateManager>>
let events_out = ctx.events_out(); // Option<EventsOutTx>
```

### Passing Through Layers

Prefer passing `&AppContext` to functions that need config/state/events:

```rust
pub async fn run_app_with_config(
    args: Args,
    run_args: Option<RunArgs>,
    recover_run_id: Option<String>,
    ctx: &AppContext,
) -> Result<i32, RunnerError> {
    let mut cfg = ctx.cfg().clone();
    let state_manager = ctx.state_manager();
    let events_out = ctx.events_out();
    // ...
}
```

### Subscribing to State Events

Each component can subscribe independently:

```rust
let mut rx = manager.subscribe();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        // handle StateEvent
    }
});
```

---

## End-User Guide: CLI Usage

### Enable State Management

Set the environment variable:

```powershell
$env:MEMEX_ENABLE_STATE_MGMT="true"
```

Then run normally:

```powershell
cargo run -- run --backend codecli --prompt "hello"
```

### Enable via Config File

Add to `config.toml` (either `~/.memex/config.toml` or `./config.toml`):

```toml
[state_management]
enabled = true
```

Notes:
- `MEMEX_ENABLE_STATE_MGMT=true` overrides the config value.

### Expected Log Output

With `MEMEX_ENABLE_STATE_MGMT=true`, CLI logs state transitions (example):

```
Session created: <session_id>
Session <session_id> -> Initializing
Session <session_id> -> MemorySearch
Session <session_id> -> RunnerStarting
Session <session_id> -> RunnerRunning
Session <session_id> -> GatekeeperEvaluating
Session <session_id> -> MemoryPersisting
Session <session_id> completed (exit=0, 2500ms)
```

### Events Emitted

The state system emits:
- `SessionCreated`
- `SessionStateChanged`
- `ToolEventReceived`
- `MemoryHit`
- `GatekeeperDecision`
- `SessionCompleted`
- `SessionFailed`

These are delivered via `StateManager::subscribe()` for custom logging/monitoring.

---

## Notes

- State management is opt-in. If `MEMEX_ENABLE_STATE_MGMT` is not `true`, no global
  `StateManager` is created and no state events are emitted.
- `AppContext` is a core type. Avoid adding `cli`-only dependencies to it.
