# TUI 立即退出问题分析

## 问题描述
UI 出现后立刻退出

## 根本原因分析

### 可能的原因 1: 渲染时机问题 ✅ 最可能

在 [flow_tui.rs#L72](cli/src/flow/flow_tui.rs#L72) 添加的渲染调用存在时序问题：

```rust
// Render once to show the pending state before starting the query
if let Err(e) = tui.terminal.draw(|f| crate::tui::ui::draw(f, &mut tui.app)) {
    tui.restore();
    return Err(RunnerError::Spawn(format!("failed to render TUI: {}", e)));
}
```

**问题流程**:
1. `prompt_for_input` 完成 → 终端在原始模式
2. 设置 `pending_qa = true`
3. **立即渲染** → 画面出现
4. 调用 `run_with_query`
5. 在 `run_with_query` 内部，会经历复杂的异步操作
6. 期间终端可能被恢复或状态改变
7. 用户看到画面闪现

**修复方向**: 这个渲染调用实际上是多余的，因为：
- 真正的 TUI 事件循环会在 `run_tui_session` → `run_with_tui_on_terminal` 中启动
- 事件循环会持续渲染
- 这个单次渲染只会造成闪烁

### 可能的原因 2: run_task 立即完成

在 [loop_run.rs#L52-L66](cli/src/tui/loop_run.rs#L52-L66) 中：

```rust
res = &mut run_task => {
    // run_task 完成
    if let Ok(ref result) = res {
        if !app.is_done() {
            app.status = super::app::RunStatus::Completed(result.exit_code);
        }
    }
    run_result = Some(res);
}
// ...
if app.is_done() || exit_requested {
    break;  // 立即退出循环
}
```

如果 `run_task` 因为错误立即返回，TUI 会检测到 `app.is_done()` 并退出。

**可能导致立即完成的情况**:
- 后端二进制不存在
- 后端启动失败
- 配置错误
- 网络连接失败（aiservice 模式）

### 可能的原因 3: 终端恢复时机问题

在 [flow_tui.rs#L103](cli/src/flow/flow_tui.rs#L103) 中：

```rust
.await;
tui.restore();  // 总是会调用，即使出错
result
```

如果有任何错误，`tui.restore()` 会被调用，终端恢复到正常模式。

## 调试建议

### 1. 移除不必要的渲染
删除 [flow_tui.rs#L72-L75](cli/src/flow/flow_tui.rs#L72-L75) 的立即渲染：

```rust
// 删除这段代码 - 会导致闪烁
// if let Err(e) = tui.terminal.draw(|f| crate::tui::ui::draw(f, &mut tui.app)) {
//     tui.restore();
//     return Err(RunnerError::Spawn(format!("failed to render TUI: {}", e)));
// }
```

**原因**: TUI 事件循环会在启动后立即开始渲染，不需要提前渲染。

### 2. 添加调试日志

在关键位置添加日志：

```rust
// 在 run_tui_session 开始
tracing::debug!("TUI session starting, run_id={}", run_id);

// 在 run_with_tui_on_terminal 开始
tracing::debug!("TUI event loop starting");

// 在 run_task 完成时
tracing::debug!("run_task completed: {:?}", res);

// 在退出循环时
tracing::debug!("Exiting TUI loop: is_done={}, exit_requested={}", app.is_done(), exit_requested);
```

### 3. 检查初始状态

确保 `TuiApp::new()` 初始化的状态正确：
- `status: RunStatus::Running` ✅
- `pending_qa: false` → 应该在启动时设置为 true ❓

### 4. 延迟检查

在渲染后添加短暂延迟，确保用户能看到画面：

```rust
// 仅用于调试
tokio::time::sleep(Duration::from_secs(2)).await;
```

## 推荐修复方案

### 方案 1: 移除提前渲染（推荐）⭐

删除 [flow_tui.rs#L72-L75](cli/src/flow/flow_tui.rs#L72-L75) 的代码：

```rust
tui.app.input_buffer.clear();
tui.app.input_cursor = 0;
tui.app.pending_qa = true;
tui.app.qa_started_at = Some(std::time::Instant::now());

// 删除这段 - 不需要提前渲染
// if let Err(e) = tui.terminal.draw(|f| crate::tui::ui::draw(f, &mut tui.app)) {
//     tui.restore();
//     return Err(RunnerError::Spawn(format!("failed to render TUI: {}", e)));
// }

if input.len() > 0 {
    // 直接进入 run_with_query
```

**原因**: 
- TUI 事件循环会在 `run_tui_session` 中启动
- 事件循环会持续渲染
- 提前渲染只会造成不必要的闪烁

### 方案 2: 确保 pending_qa 状态在事件循环中可见

修改状态设置时机，在 `run_tui_session` 开始时确认状态：

```rust
pub async fn run_tui_session(
    tui: &mut TuiRuntime,
    run_id: String,
    // ... 其他参数
) -> Result<RunnerResult, RunnerError> {
    // 确保 pending_qa 状态正确
    // （实际上已经在外部设置了，这里只是确认）
    
    let (tui_tx, tui_rx) = mpsc::unbounded_channel();
    // ... 继续
```

### 方案 3: 添加启动延迟（仅用于调试）

```rust
// 在 run_with_tui_on_terminal 开始时
pub async fn run_with_tui_on_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut TuiApp,
    mut event_rx: mpsc::UnboundedReceiver<TuiEvent>,
    mut run_task: tokio::task::JoinHandle<Result<RunnerResult, RunnerError>>,
) -> Result<RunnerResult, RunnerError> {
    let (input_reader, mut input_rx) = InputReader::start();
    let mut tick =
        tokio::time::interval(Duration::from_millis(app.config.update_interval_ms.max(16)));

    // 立即渲染一次以显示初始状态
    terminal
        .draw(|f| ui::draw(f, app))
        .map_err(|e| RunnerError::Spawn(e.to_string()))?;

    let mut run_result: Option<Result<RunnerResult, RunnerError>> = None;
    // ... 继续
```

## 执行计划

1. **首先**: 移除 flow_tui.rs 中的提前渲染代码
2. **然后**: 测试 TUI 是否正常工作
3. **如果还有问题**: 添加调试日志查看具体哪里出错
4. **最后**: 根据日志信息进一步诊断

## 代码位置参考

- 问题代码: [flow_tui.rs#L72-L75](cli/src/flow/flow_tui.rs#L72-L75)
- TUI 事件循环: [loop_run.rs#L30-L88](cli/src/tui/loop_run.rs#L30-L88)
- 状态检查: [app.rs#L195-L197](cli/src/tui/app.rs#L195-L197)
- 终端恢复: [flow_tui.rs#L103](cli/src/flow/flow_tui.rs#L103)
