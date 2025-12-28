# TUI 设计方案

## 一、概述

当 CLI 参数 `--stream-format=text` 时，启用交互式 TUI（Terminal User Interface）模式，提供更丰富的实时流式输出体验。

## 二、触发条件

- `--stream` 为 `true`
- `--stream-format` 为 `"text"`（默认值）

## 三、技术选型

### 推荐库：`ratatui`

**理由：**
- 现代化的 Rust TUI 框架，基于 `crossterm`
- 异步友好，与 `tokio` 无缝集成
- 活跃维护，文档完善
- 支持丰富的组件：块、列表、表格、图表等
- 跨平台支持（Windows/Linux/macOS）

**依赖：**
```toml
ratatui = "0.28"
crossterm = "0.28"
```

### 备选方案：
- `cursive`：更高层次的抽象，但不太适合流式数据展示
- `tui-rs`：已弃用，ratatui 是其继任者

## 四、架构设计

### 4.1 模块结构

```
cli/src/
  tui/
    mod.rs           - TUI 模块入口
    app.rs           - TUI 应用状态管理
    ui.rs            - UI 布局渲染
    events.rs        - 事件处理（键盘、鼠标）
    widgets/         - 自定义 widget
      tool_event.rs  - 工具事件展示组件
      output.rs      - 输出流展示组件
```

### 4.2 集成点

修改 `cli/src/app.rs`：

```rust
let stream_format = run_args
    .as_ref()
    .map(|ra| ra.stream_format.as_str())
    .unwrap_or("text");

let should_use_tui = run_args
    .as_ref()
    .map(|ra| ra.stream && ra.stream_format == "text")
    .unwrap_or(false);

if should_use_tui {
    // 进入 TUI 模式
    return crate::tui::run_with_tui(args, run_args, cfg).await;
}

// 原有逻辑
let stream = factory::build_stream(stream_format);
// ...
```

## 五、UI 布局设计

### 5.1 整体布局

```
┌─────────────────────────────────────────────────────────────────────┐
│ Memex CLI - Run ID: abc123-456... │ Status: Running │ Token: 12345 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│ ┌──────────────────────── Tool Events ─────────────────────────┐    │
│ │ [1] 12:34:56 tool_call edit_file                             │    │
│ │     args: {"file": "main.rs", "line": 10}                    │    │
│ │ [2] 12:34:57 tool_result success                             │    │
│ │     output: "File edited successfully"                       │    │
│ │ [3] 12:34:58 tool_call run_command                           │    │
│ │     args: {"cmd": "cargo test"}                              │    │
│ │ [4] 12:35:02 tool_result success                             │    │
│ │     output: "All tests passed"                               │    │
│ └──────────────────────────────────────────────────────────────┘    │
│                                                                       │
│ ┌───────────────────── Assistant Output ───────────────────────┐    │
│ │ I'll help you with that task...                              │    │
│ │                                                               │    │
│ │ First, I'll edit the main file...                            │    │
│ │ [Tool call: edit_file]                                       │    │
│ │                                                               │    │
│ │ Now running tests...                                         │    │
│ │ [Tool call: run_command]                                     │    │
│ │                                                               │    │
│ │ All tests passed successfully!                               │    │
│ └──────────────────────────────────────────────────────────────┘    │
│                                                                       │
│ ┌────────────────────── Raw Output ────────────────────────────┐    │
│ │ stdout: Running test suite...                                │    │
│ │ stdout: test_basic ... ok                                    │    │
│ │ stdout: test_advanced ... ok                                 │    │
│ └──────────────────────────────────────────────────────────────┘    │
│                                                                       │
├─────────────────────────────────────────────────────────────────────┤
│ [q] Quit  [↑↓] Scroll  [Tab] Switch Panel  [p] Pause  [c] Copy     │
└─────────────────────────────────────────────────────────────────────┤
```

### 5.2 布局分区

#### 顶部状态栏（Header）
- **Run ID**：当前运行 ID
- **Status**：运行状态（Running / Paused / Completed / Error）
- **Metrics**：实时统计（Token 数、工具调用次数、运行时长）

#### 主内容区域（3 个可切换面板）
1. **Tool Events 面板**
   - 显示所有工具调用和结果
   - 支持折叠/展开详细参数
   - 高亮显示错误/警告
   - 自动滚动到最新事件

2. **Assistant Output 面板**
   - 显示 AI 助手的流式输出
   - 语法高亮（Markdown 支持）
   - 支持代码块渲染

3. **Raw Output 面板**
   - 原始 stdout/stderr 输出
   - 分色显示（stdout 白色，stderr 红色）
   - 支持正则搜索/过滤

#### 底部快捷键栏（Footer）
- 常用快捷键提示
- 可配置隐藏

### 5.3 交互设计

#### 键盘快捷键
- `q` / `Ctrl+C`：退出
- `↑` / `↓`：滚动当前面板
- `PgUp` / `PgDn`：翻页
- `Home` / `End`：跳到开始/结束
- `Tab` / `Shift+Tab`：切换面板
- `p`：暂停/恢复输出流
- `/`：进入搜索模式
- `c`：复制选中内容到剪贴板
- `f`：进入过滤模式
- `Space`：展开/折叠 Tool Event 详情
- `r`：刷新/重绘界面

#### 鼠标支持（可选）
- 滚轮滚动
- 点击切换面板
- 拖拽调整分区大小

## 六、数据流设计

### 6.1 事件传递机制

```rust
// 使用 tokio channel 在 runner 和 TUI 之间传递事件
pub enum TuiEvent {
    ToolEvent(ToolEventLite),
    AssistantOutput(String),
    RawStdout(Vec<u8>),
    RawStderr(Vec<u8>),
    StatusUpdate { tokens: u64, duration: Duration },
    RunComplete { exit_code: i32 },
    Error(String),
}

// TUI App 持有接收端
struct TuiApp {
    event_rx: mpsc::UnboundedReceiver<TuiEvent>,
    tool_events: Vec<ToolEventLite>,
    assistant_buffer: String,
    stdout_buffer: RingBytes,
    stderr_buffer: RingBytes,
    status: RunStatus,
    // ...
}
```

### 6.2 Runner 集成

在 `core/src/runner/run.rs` 中，除了现有的 `events_out_tx`，增加 `tui_tx`：

```rust
pub async fn run_session(
    mut session: Box<dyn RunnerSession>,
    control: &ControlConfig,
    policy: Option<Box<dyn PolicyPlugin>>,
    capture_bytes: usize,
    events_out: Option<EventsOutTx>,
    tui_tx: Option<mpsc::UnboundedSender<TuiEvent>>, // 新增
    run_id: &str,
    silent: bool,
) -> Result<RunnerResult, RunnerError> {
    // ...
    
    // 在解析到 tool event 时发送到 TUI
    if let Some(ref tx) = tui_tx {
        let _ = tx.send(TuiEvent::ToolEvent(event.clone()));
    }
    
    // 在 stdout/stderr tee 时发送到 TUI
    if let Some(ref tx) = tui_tx {
        let _ = tx.send(TuiEvent::RawStdout(chunk.to_vec()));
    }
    
    // ...
}
```

### 6.3 异步渲染循环

```rust
async fn run_tui_loop(
    app: &mut TuiApp,
    terminal: &mut Terminal<impl Backend>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tick_interval = tokio::time::interval(Duration::from_millis(50));
    
    loop {
        tokio::select! {
            // 处理 TUI 事件
            Some(event) = app.event_rx.recv() => {
                app.handle_tui_event(event);
                terminal.draw(|f| ui::draw(f, app))?;
            }
            
            // 处理用户输入
            Ok(true) = poll_user_input() => {
                if let Some(input) = read_user_input()? {
                    if app.handle_user_input(input) {
                        break; // 用户退出
                    }
                    terminal.draw(|f| ui::draw(f, app))?;
                }
            }
            
            // 定时刷新（处理动画、状态更新等）
            _ = tick_interval.tick() => {
                app.tick();
                terminal.draw(|f| ui::draw(f, app))?;
            }
        }
    }
    
    Ok(())
}
```

## 七、状态管理

### 7.1 应用状态

```rust
pub struct TuiApp {
    // 数据源
    event_rx: mpsc::UnboundedReceiver<TuiEvent>,
    
    // 内容缓冲
    tool_events: Vec<ToolEventLite>,
    assistant_buffer: String,
    stdout_buffer: RingBytes,
    stderr_buffer: RingBytes,
    
    // UI 状态
    active_panel: PanelKind,
    scroll_offset: usize,
    paused: bool,
    filter: Option<Regex>,
    
    // 运行状态
    status: RunStatus,
    run_id: String,
    start_time: Instant,
    token_count: u64,
    tool_call_count: usize,
    
    // 搜索状态
    search_mode: bool,
    search_query: String,
    search_results: Vec<SearchResult>,
}

pub enum PanelKind {
    ToolEvents,
    AssistantOutput,
    RawOutput,
}

pub enum RunStatus {
    Initializing,
    Running,
    Paused,
    Completed(i32),
    Error(String),
}
```

### 7.2 事件处理

```rust
impl TuiApp {
    fn handle_tui_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::ToolEvent(evt) => {
                self.tool_events.push(evt);
                self.tool_call_count += 1;
                if !self.paused {
                    self.auto_scroll();
                }
            }
            TuiEvent::AssistantOutput(text) => {
                self.assistant_buffer.push_str(&text);
                if !self.paused {
                    self.auto_scroll();
                }
            }
            TuiEvent::RawStdout(chunk) => {
                self.stdout_buffer.push(&chunk);
            }
            TuiEvent::RawStderr(chunk) => {
                self.stderr_buffer.push(&chunk);
            }
            TuiEvent::StatusUpdate { tokens, .. } => {
                self.token_count = tokens;
            }
            TuiEvent::RunComplete { exit_code } => {
                self.status = RunStatus::Completed(exit_code);
            }
            TuiEvent::Error(msg) => {
                self.status = RunStatus::Error(msg);
            }
        }
    }
    
    fn handle_user_input(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') => return true, // 退出
            KeyCode::Up => self.scroll_up(),
            KeyCode::Down => self.scroll_down(),
            KeyCode::Tab => self.next_panel(),
            KeyCode::Char('p') => self.toggle_pause(),
            KeyCode::Char('/') => self.enter_search_mode(),
            KeyCode::Char('c') => self.copy_selection(),
            // ...
            _ => {}
        }
        false
    }
}
```

## 八、渲染实现

### 8.1 主渲染函数

```rust
pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Main content
            Constraint::Length(1),  // Footer
        ])
        .split(f.size());
    
    draw_header(f, chunks[0], app);
    draw_main_content(f, chunks[1], app);
    draw_footer(f, chunks[2], app);
}
```

### 8.2 组件示例：Tool Events 面板

```rust
fn draw_tool_events<B: Backend>(f: &mut Frame<B>, area: Rect, app: &TuiApp) {
    let items: Vec<ListItem> = app
        .tool_events
        .iter()
        .enumerate()
        .map(|(i, evt)| {
            let icon = match evt.event_type.as_str() {
                "tool_call" => "🔧",
                "tool_result" => "✅",
                _ => "•",
            };
            
            let timestamp = format_timestamp(&evt.timestamp);
            let name = evt.name.clone().unwrap_or_default();
            
            let line = format!(
                "[{}] {} {} {}",
                i + 1, timestamp, icon, name
            );
            
            ListItem::new(line).style(Style::default().fg(Color::Cyan))
        })
        .collect();
    
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Tool Events")
                .border_type(BorderType::Rounded)
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    
    f.render_widget(list, area);
}
```

## 九、性能优化

### 9.1 缓冲管理
- 使用 `RingBytes` 限制内存占用
- Tool events 超过一定数量时启用分页或虚拟滚动
- Assistant output 超长时只渲染可见区域

### 9.2 渲染优化
- 仅在有新数据或用户交互时重绘
- 使用差分渲染减少终端 I/O
- 暂停模式下停止自动滚动和刷新

### 9.3 异步处理
- 事件处理和渲染分离
- 使用 `tokio::select!` 避免阻塞
- 用户输入采用非阻塞 polling

## 十、错误处理与降级

### 10.1 终端兼容性检测
```rust
pub fn check_tui_support() -> Result<(), String> {
    if !atty::is(atty::Stream::Stdout) {
        return Err("stdout is not a terminal".into());
    }
    
    if std::env::var("TERM").is_err() {
        return Err("TERM environment variable not set".into());
    }
    
    // 检测终端尺寸
    let (width, height) = crossterm::terminal::size()
        .map_err(|e| format!("failed to get terminal size: {}", e))?;
    
    if width < 80 || height < 24 {
        return Err(format!(
            "terminal too small ({}x{}), need at least 80x24",
            width, height
        ));
    }
    
    Ok(())
}
```

### 10.2 降级策略
如果 TUI 初始化失败，自动降级为普通文本流式输出：

```rust
pub async fn run_with_tui_or_fallback(
    args: Args,
    run_args: Option<RunArgs>,
    cfg: AppConfig,
) -> Result<i32, RunnerError> {
    match check_tui_support() {
        Ok(_) => {
            match run_with_tui(args, run_args, cfg).await {
                Ok(code) => Ok(code),
                Err(e) => {
                    eprintln!("TUI failed, falling back to text mode: {}", e);
                    run_with_text_stream(args, run_args, cfg).await
                }
            }
        }
        Err(reason) => {
            tracing::debug!("TUI not supported: {}", reason);
            run_with_text_stream(args, run_args, cfg).await
        }
    }
}
```

## 十一、配置选项

在 `config.toml` 中支持 TUI 配置：

```toml
[tui]
enabled = true
auto_scroll = true
show_timestamps = true
show_raw_output = true
color_scheme = "default"  # default / dark / light
update_interval_ms = 50
max_tool_events = 1000
max_output_lines = 10000

[tui.keybindings]
quit = ["q", "Ctrl+C"]
scroll_up = ["Up", "k"]
scroll_down = ["Down", "j"]
next_panel = ["Tab"]
prev_panel = ["Shift+Tab"]
pause = ["p", "Space"]
search = ["/"]
```

## 十二、测试策略

### 12.1 单元测试
- 事件处理逻辑
- 状态转换
- 缓冲管理

### 12.2 集成测试
- 模拟流式数据输入
- 验证面板切换
- 测试暂停/恢复

### 12.3 手动测试
- 不同终端模拟器（Windows Terminal, iTerm2, Alacritty）
- 不同终端尺寸
- 长时间运行稳定性

## 十三、实施计划

### Phase 1：基础框架（1-2 天）
- [ ] 添加 `ratatui` 和 `crossterm` 依赖
- [ ] 创建 `tui` 模块结构
- [ ] 实现基础 TUI 初始化和退出逻辑
- [ ] 实现简单的三面板布局

### Phase 2：数据集成（2-3 天）
- [ ] 在 `runner` 中添加 `tui_tx` channel
- [ ] 实现事件从 runner 到 TUI 的传递
- [ ] 实现 Tool Events 面板数据展示
- [ ] 实现 Raw Output 面板数据展示

### Phase 3：交互功能（1-2 天）
- [ ] 实现键盘快捷键
- [ ] 实现滚动和面板切换
- [ ] 实现暂停/恢复功能
- [ ] 实现搜索/过滤功能

### Phase 4：优化与完善（1-2 天）
- [ ] 性能优化（虚拟滚动、差分渲染）
- [ ] 错误处理和降级逻辑
- [ ] 配置文件支持
- [ ] 文档和测试

### Phase 5：高级特性（可选）
- [ ] 鼠标支持
- [ ] 语法高亮
- [ ] 主题支持
- [ ] 导出功能（保存到文件）

## 十四、示例用法

```bash
# 启用 TUI 模式（默认）
memex-cli run --backend codex --prompt "Hello" --stream

# 显式指定 TUI 模式
memex-cli run --backend codex --prompt "Hello" --stream --stream-format text

# 禁用 TUI，使用 JSONL 模式
memex-cli run --backend codex --prompt "Hello" --stream --stream-format jsonl
```

## 十五、未来扩展

### 15.1 高级可视化
- 添加工具调用依赖图
- 添加性能监控图表（Token/秒）
- 添加内存使用监控

### 15.2 协作功能
- 多用户查看同一个 run
- 实时共享 TUI session

### 15.3 回放模式
- 在 `replay` 命令中支持 TUI
- 支持时间轴导航
- 支持暂停/单步调试

---

## 附录 A：依赖项

```toml
# cli/Cargo.toml
[dependencies]
ratatui = "0.28"
crossterm = "0.28"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
tracing = "0.1"
```

## 附录 B：参考资源

- [Ratatui 官方文档](https://ratatui.rs/)
- [Ratatui Examples](https://github.com/ratatui-org/ratatui/tree/main/examples)
- [Crossterm 文档](https://docs.rs/crossterm/)
- [类似项目参考]
  - `k9s`（Kubernetes TUI）
  - `lazygit`（Git TUI）
  - `bottom`（系统监控 TUI）
