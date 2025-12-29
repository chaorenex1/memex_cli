# TUI 设计方案

## 一、概述

当 CLI 参数 `--stream-format=text` 时，启用交互式 TUI（Terminal User Interface）模式，提供更丰富的实时流式输出体验。

## 二、触发条件

- `--tui` 为 `true`
- `--stream-format` 为 `"text"`（TUI 会强制为 text）

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
    splash.rs        - 启动画面
    widgets/         - 自定义 widget
      tool_event.rs  - 工具事件展示组件
      output.rs      - 输出流展示组件
      banner.rs      - ASCII 艺术字和标语
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
    .map(|ra| ra.tui && ra.stream_format == "text")
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

### 5.0 启动画面（Splash Screen）

TUI 启动时显示品牌化的启动画面，停留 1-2 秒后自动进入主界面：

```
╭─────────────────────────────────────────────────────────────────────╮
│                                                                       │
│                                                                       │
│        __  __                                                         │
│        |  \/  | ___ _ __ ___   _____  __                             │
│        | |\/| |/ _ \ '_ ` _ \ / _ \ \/ /                             │
│        | |  | |  __/ | | | | |  __/>  <                              │
│        |_|  |_|\___|_| |_| |_|\___/_/\_\  CLI                        │
│        --------------------------------------                        │
│         > Memory Layer & Code Engine Wrapper                         │
│                                                                       │
│                                                                       │
│                   🚀 Initializing Memex CLI...                       │
│                                                                       │
│                      Version: 0.1.0                                  │
│                      Status: Streaming | Gatekeeper: ON              │
│                                                                       │
│                                                                       │
│                   Loading configuration... ✓                         │
│                   Connecting to backend... ✓                         │
│                   Starting TUI interface...                          │
│                                                                       │
│                                                                       │
│                                                                       │
╰─────────────────────────────────────────────────────────────────────╯
```

**启动流程：**
1. 显示 ASCII Art Logo（0.5s）
2. 显示版本和状态信息（0.5s）
3. 显示加载进度（实时）
4. 加载完成后淡出进入主界面（0.5s）

**可配置项：**
- `tui.show_splash` - 是否显示启动画面（默认：true）
- `tui.splash_duration_ms` - 最小停留时间（默认：1500ms）
- `tui.splash_animation` - 启用加载动画（默认：true）

### 5.1 整体布局

```
╭─────────────────────────────────────────────────────────────────────╮
│ ◉ Memex CLI          Run: abc123-456    ⚡ Running    🔧 4  💬 12345 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  Tool Events                                                     [1] │
│  ─────────────────────────────────────────────────────────────────  │
│                                                                       │
│  🔧 12:34:56  edit_file                                              │
│  │ file: "main.rs", line: 10                                        │
│  ✅ 12:34:57  success → File edited successfully                    │
│                                                                       │
│  🔧 12:34:58  run_command                                            │
│  │ cmd: "cargo test"                                                │
│  ✅ 12:35:02  success → All tests passed                            │
│                                                                       │
│  ─────────────────────────────────────────────────────────────────  │
│                                                                       │
│  Assistant Output                                                [2] │
│  ─────────────────────────────────────────────────────────────────  │
│                                                                       │
│  I'll help you with that task...                                    │
│                                                                       │
│  First, I'll edit the main file...                                  │
│  → [Tool: edit_file]                                                │
│                                                                       │
│  Now running tests...                                               │
│  → [Tool: run_command]                                              │
│                                                                       │
│  ✓ All tests passed successfully!                                   │
│                                                                       │
│  ─────────────────────────────────────────────────────────────────  │
│                                                                       │
│  Raw Output                                                      [3] │
│  ─────────────────────────────────────────────────────────────────  │
│                                                                       │
│  Running test suite...                                              │
│  test_basic ... ok                                                  │
│  test_advanced ... ok                                               │
│                                                                       │
├─────────────────────────────────────────────────────────────────────┤
│ > _                                                                   │
│ Normal Mode  ⌨ Press : for commands, / for search, Tab to switch    │
╰─────────────────────────────────────────────────────────────────────╯
```

### 5.2 布局分区

#### 顶部状态栏（Header）
- **应用标识**：`◉ Memex CLI` - 带图标的品牌标识
- **Run ID**：显示当前运行的简短 ID
- **状态指示器**：
  - `⚡ Running` - 运行中
  - `⏸ Paused` - 已暂停
  - `✓ Completed` - 已完成
  - `✗ Error` - 出错
- **实时指标**：
  - `🔧 N` - 工具调用次数
  - `💬 N` - Token 计数
  - `⏱ MM:SS` - 运行时长（可选）

#### 主内容区域（3 个面板，分屏显示）
所有面板同时可见，采用现代化无边框设计，通过分隔线区分。

1. **Tool Events 面板** `[1]`
   - 使用图标标识：`🔧` 工具调用，`✅` 成功，`❌` 失败
   - 简洁的树状展示结构
   - 参数缩进显示，避免过度嵌套
   - 支持展开/折叠（按空格键）
   - 高亮最新事件

2. **Assistant Output 面板** `[2]`
   - 流式显示 AI 助手输出
   - 使用 `→` 箭头标识工具调用
   - 使用 `✓` 标识完成状态
   - 支持 Markdown 语法（粗体、代码块等）
   - 自动换行和智能缩进

3. **Raw Output 面板** `[3]`
   - 原始 stdout/stderr 混合显示
   - stdout 使用默认颜色
   - stderr 使用红色/橙色高亮
   - 可通过输入框过滤内容

#### 底部输入区域（Input Bar）
现代化的多功能输入框，替代传统快捷键栏：

- **主输入框**：`> _` - 光标闪烁
- **模式指示器**：显示当前输入模式
  - `Normal Mode` - 普通模式（接收单键命令）
  - `Command Mode` - 命令模式（输入 `:` 进入）
  - `Search Mode` - 搜索模式（输入 `/` 进入）
  - `Filter Mode` - 过滤模式（输入 `?` 进入）
- **提示文本**：简短的操作提示，右对齐显示

### 5.3 交互设计

#### 输入模式系统
受 Vim 启发的现代化输入模式设计：

##### 1. Normal Mode（普通模式）- 默认模式
单键快捷操作：
- `q` / `Ctrl+C`：退出应用
- `j` / `↓`：向下滚动当前面板
- `k` / `↑`：向上滚动当前面板
- `h` / `←`：滚动到行首
- `l` / `→`：滚动到行尾
- `Ctrl+D`：向下翻页
- `Ctrl+U`：向上翻页
- `g g`：跳到开始（连按两次 g）
- `G`：跳到末尾
- `Tab`：切换到下一面板
- `1` / `2` / `3`：直接切换到面板 1/2/3
- `p`：暂停/恢复输出流
- `Space`：展开/折叠当前 Tool Event
- `y`：复制当前行到剪贴板
- `Y`：复制整个面板内容

进入其他模式：
- `:`：进入命令模式
- `/`：进入搜索模式
- `?`：进入过滤模式
- `i`：进入输入模式（用于发送消息，未来扩展）

##### 2. Command Mode（命令模式）
输入 `:` 后进入，可执行命令：
- `:q` 或 `:quit` - 退出
- `:w <file>` 或 `:write <file>` - 保存当前面板到文件
- `:export <file>` - 导出所有数据到文件
- `:clear` - 清空当前面板内容
- `:pause` - 暂停输出
- `:resume` - 恢复输出
- `:theme <name>` - 切换主题
- `:help` - 显示帮助信息
- `:panel <1|2|3>` - 切换面板
- `Esc` - 返回 Normal Mode

##### 3. Search Mode（搜索模式）
输入 `/` 后进入，可搜索内容：
- 输入搜索词，实时高亮匹配项
- `Enter` - 跳到下一个匹配
- `Shift+Enter` - 跳到上一个匹配
- `n` - 下一个匹配（搜索后在 Normal Mode 使用）
- `N` - 上一个匹配
- `Esc` - 返回 Normal Mode

##### 4. Filter Mode（过滤模式）
输入 `?` 后进入，可过滤显示内容：
- 输入正则表达式或关键词
- 实时过滤当前面板内容
- `Enter` - 应用过滤
- `Esc` - 清除过滤，返回 Normal Mode

#### 可视化反馈
- **光标**：输入框中显示闪烁光标
- **高亮**：当前活动面板使用不同颜色边框
- **动画**：新内容到达时短暂闪烁
- **进度**：长时间操作显示 spinner 动画
- **通知**：操作完成后在输入框上方显示提示（2秒后消失）

#### 鼠标支持（可选）
- 滚轮滚动当前鼠标所在面板
- 点击面板切换激活状态
- 点击输入框进入输入模式
- 拖拽面板边界调整大小（高级特性）

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
    
    // 初始绘制启动画面
    terminal.draw(|f| ui::draw(f, app))?;
    
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
                    // 启动画面期间禁用用户输入
                    if !app.is_initializing() && app.handle_user_input(input) {
                        break; // 用户退出
                    }
                    terminal.draw(|f| ui::draw(f, app))?;
                }
            }
            
            // 定时刷新（处理动画、状态更新等）
            _ = tick_interval.tick() => {
                // 更新启动进度
                if app.is_initializing() {
                    app.update_splash_progress();
                }
                
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
    expanded_events: HashSet<usize>, // 展开的事件索引
    
    // 输入状态
    input_mode: InputMode,
    input_buffer: String,
    cursor_pos: usize,
    command_history: Vec<String>,
    history_index: usize,
    
    // 运行状态
    status: RunStatus,
    run_id: String,
    start_time: Instant,
    token_count: u64,
    tool_call_count: usize,
    
    // 启动状态
    is_splash_showing: bool,
    splash_progress: f32,
    splash_start_time: Instant,
    
    // 搜索状态
    search_query: String,
    search_results: Vec<SearchResult>,
    current_search_index: usize,
    
    // 通知状态
    notification: Option<Notification>,
}

pub enum InputMode {
    Normal,    // 普通模式（单键命令）
    Command,   // 命令模式（输入 : 进入）
    Search,    // 搜索模式（输入 / 进入）
    Filter,    // 过滤模式（输入 ? 进入）
}

pub struct Notification {
    message: String,
    level: NotificationLevel,
    expires_at: Instant,
}

pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

pub enum PanelKind {
    ToolEvents,
    AssistantOutp  // 启动中（显示 splash）
    Running,       // 正常运行
    Paused,        // 已暂停
    Completed(i32),// 已完成（退出码）
    Error(String), // 出错
}

impl TuiApp {
    pub fn is_initializing(&self) -> bool {
        matches!(self.status, RunStatus::Initializing) && self.is_splash_showing
    }
    
    pub fn update_splash_progress(&mut self) {
        let elapsed = self.splash_start_time.elapsed();
        let min_duration = Duration::from_millis(1500);
        
        // 根据实际初始化进度和时间计算进度
        self.splash_progress = (elapsed.as_millis() as f32 / min_duration.as_millis() as f32)
            .min(1.0);
        
        // 进度达到 100% 后关闭启动画面
        if self.splash_progress >= 1.0 && matches!(self.status, RunStatus::Initializing) {
            self.is_splash_showing = false;
            self.status = RunStatus::Running;
        }
    } {
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
              self.input_mode {
            InputMode::Normal => self.handle_normal_mode(key),
            InputMode::Command => self.handle_command_mode(key),
            InputMode::Search => self.handle_search_mode(key),
            InputMode::Filter => self.handle_filter_mode(key),
        }
    }
    
    fn handle_normal_mode(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => return true, // 退出
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
            KeyCode::Char('h') | KeyCode::Left => self.scroll_to_start(),
            KeyCode::Char('l') | KeyCode::Right => self.scroll_to_end(),
            KeyCode::Char('g') => {
                if self.last_key == Some('g') {
                    self.scroll_to_top();
                }
                self.last_key = Some('g');
            }
            KeyCode::Char('G') => self.scroll_to_bottom(),
            KeyCode::Tab => self.next_panel(),
            KeyCode::BackTab => self.prev_panel(),
            KeyCode::Char('1') => self.switch_to_panel(PanelKind::ToolEvents),
            KeyCode::Char('2') => self.switch_to_panel(PanelKind::AssistantOutput),
            KeyCode::Char('3') => self.switch_to_panel(PanelKind::RawOutput),
            KeyCode::Char('p') => self.toggle_pause(),
            KeyCode::Char(' ') => self.toggle_expand_current(),
            KeyCode::Char('y') => self.copy_current_line(),
            KeyCode::Char('Y') => self.copy_panel_content(),
            KeyCode::Char(':') => self.enter_command_mode(),
            KeyCode::Char('/') => self.enter_search_mode(),
            KeyCode::Char('?') => self.enter_filter_mode(),
            KeyCode::Char('n') => self.search_next(),
            KeyCode::Char('N') => self.search_prev(),
            KeyCode::Ctrl('d') => self.page_down(),
            KeyCode::Ctrl('u') => self.page_up(),
            _ => {
                self.last_key = None;
            }
        }
        false
    }
    
    fn handle_command_mode(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.exit_input_mode();
            }
            KeyCode::Enter => {
                let should_quit = self.execute_command();
                self.exit_input_mode();
                return should_quit;
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.input_buffer.remove(self.cursor_pos - 1);
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Delete => {
                if self.cursor_pos < self.input_buffer.len() {
                    self.input_buffer.remove(self.cursor_pos);
                }
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_pos < self.input_buffer.len() {
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = self.input_buffer.len();
            }
            KeyCode::Up => {
                self.history_prev();
            }
            KeyCode::Down => {
                self.history_next();
            }
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.cursor_pos, c);
                self.cursor_pos += 1;
            }
            _ => {}
        }
        false
    }
    
    fn handle_search_mode(&mut self, key: KeyEvent) -> bool {
        // 类似 command_mode，但 Enter 时执行搜索
        match key.code {
            KeyCode::Esc => {
                self.exit_input_mode();
            }
            KeyCode::Enter => {
                self.perform_search();
                self.exit_input_mode();
            }
            // ... 其他按键处理同 command_mode
            _ => {}
        }
        false
    }
    
    fn handle_filter_mode(&mut self, key: KeyEvent) -> bool {
        // 类似 search_mode，但应用过滤
        match key.code {
            KeyCode::Esc => {
                self.clear_filter();
                self.exit_input_mode();
            }
            KeyCode::Enter => {
                self.apply_filter();
                self.exit_input_mode();
            }
            // ... 其他按键处理 !self.paused {
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

```r// 如果在启动状态，显示启动画面
    if app.is_initializing() {
        draw_splash_screen(f, f.size(), app);
        return;
    }
    
    // 主界面布局
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Header (compact)
            Constraint::Min(0),     // Main content (flexible)
            Constraint::Length(2),  // Input bar
        ])
        .split(f.size());
    
    draw_header(f, chunks[0], app);
    draw_main_content(f, chunks[1], app);
    draw_input_bar(f, chunks[2], app);
}

// 绘制启动画面
fn draw_splash_screen<B: Backend>(f: &mut Frame<B>, area: Rect, app: &TuiApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));
    
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    // ASCII Art Banner
    let banner = vec![
        "",
        "      __  __                      ",
        "      |  \\/  | ___ _ __ ___   _____  __",
        "      | |\\/| |/ _ \\ '_ ` _ \\ / _ \\ \\/ /",
        "      | |  | |  __/ | | | | |  __/>  < ",
        "      |_|  |_|\\___|_| |_| |_|\\___/_/\\_\\  CLI",
        "      --------------------------------------",
        "       > Memory Layer & Code Engine Wrapper",
        "",
        "",
    ];
    
    let banner_height = banner.len() as u16;
    let start_y = (inner.height.saturating_sub(banner_height + 10)) / 2;
    
    // 渲染 Banner
    for (i, line) in banner.iter().enumerate() {
        let y = inner.y + start_y + i as u16;
        if y < inner.y + inner.height {
            let banner_line = Paragraph::new(*line)
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center);
            let line_area = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            };
            f.render_widget(banner_line, line_area);
        }
    }
    
    // 状态信息
    let status_y = inner.y + start_y + banner_height + 2;
    
    // 初始化消息
    let init_msg = if app.splash_progress < 0.3 {
        "🚀 Initializing Memex CLI..."
    } else if app.splash_progress < 0.6 {
        "🚀 Loading configuration... ✓"
    } else if app.splash_progress < 0.9 {
        "🚀 Connecting to backend... ✓"
    } else {
        "🚀 Starting TUI interface..."
    };
    
    let init_line = Paragraph::new(init_msg)
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);
    f.render_widget(init_line, Rect {
        x: inner.x,
        y: status_y,
        width: inner.width,
        height: 1,
    });
    
    // 版本信息
    let version_line = Paragraph::new(format!(
        "Version: {}",
        env!("CARGO_PKG_VERSION")
    ))
    .style(Style::default().fg(Color::DarkGray))
    .alignment(Alignment::Center);
    f.render_widget(version_line, Rect {
        x: inner.x,
        y: status_y + 2,
        width: inner.width,
        height: 1,
    });
    
    // 状态信息
    let status_info = format!(
        "Status: {} | Gatekeeper: {}",
        if app.config.stream { "Streaming" } else { "Batch" },
        if app.config.gatekeeper_enabled { "ON" } else { "OFF" }
    );
    let status_line = Paragraph::new(status_info)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(status_line, Rect {
        x: inner.x,
        y: status_y + 3,
        width: inner.width,
        height: 1,
    });
    
    // 进度条（可选）
    if app.splash_progress < 1.0 {
        let progress_width = (inner.width as f32 * 0.6) as u16;
        let progress_x = inner.x + (inner.width - progress_width) / 2;
        let filled = (progress_width as f32 * app.splash_progress) as u16;
        
        let progress_bar = format!(
            "[{}{}] {:.0}%",
            "=".repeat(filled as usize),
            " ".repeat((progress_width - filled) as usize),
            app.splash_progress * 100.0
        );
        
        let progress_line = Paragraph::new(progress_bar)
            .style(Style::default().fg(Color::Green))
            .alignment(Alignment::Center);
        f.render_widget(progress_line, Rect {
            x: progress_x,
            y: status_y + 5,
            width: progress_width,
            height: 1,
        });
    }
    draw_main_content(f, chunks[1], app);
    draw_input_bar(f, chunks[2], app);
}

// 绘制输入区域
fn draw_input现代化 Tool Events 面板

```rust
fn draw_tool_events<B: Backend>(f: &mut Frame<B>, area: Rect, app: &TuiApp) {
    let is_active = app.active_panel == PanelKind::ToolEvents;
    
    // 无边框设计，使用简单分隔线
    let title = Span::styled(
        "  Tool Events",
        Style::default()
            .fg(if is_active { Color::Cyan } else { Color::Gray })
            .add_modifier(Modifier::BOLD),
    );
    
    let panel_indicator = Span::styled(
        "[1]",
        Style::default().fg(Color::DarkGray),
    );
    
    // 构建标题行
    let title_line = Line::from(vec![title, Span::raw(" "), panel_indicator]);
    
    // 构建内容
    let mut lines = vec![title_line];
    lines.push(Line::from("  ─────────────────────────────────────────────────"));
    lines.push(Line::from("")); // 空行
    
    for (i, evt) in app.tool_events.iter().enumerate() {
        if app.filter.as_ref().map_or(false, |f| !f.is_match(&evt.name)) {
            continue; // 过滤不匹配的事件
        }
        
        let icon = match evt.event_type.as_str() {
            "tool_call" => "🔧",
            "tool_result" if evt.status == Some("success") => "✅",
            "tool_result" if evt.status == Some("error") => "❌",
            _ => "•",
        };
        
        let timestamp = format_timestamp(&evt.timestamp);
        let name = evt.name.clone().unwrap_or_default();
        
        // 主行
        let main_line = Line::from(vec![
            Span::raw("  "),
            Span::styled(icon, Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::styled(timestamp, Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(name, Style::default().fg(Color::Cyan)),
        ]);
        
        lines.push(main_line);
        
        // 展开的详情（如果需要）
        if app.expanded_events.contains(&i) {
            if let Some(args) = &evt.args {
                let args_preview = format_args_preview(args, 60);
                let detail_line = Line::from(vec![
                    Span::raw("  │ "),
                    Span::styled(args_preview, Style::default().fg(Color::Gray)),
                ]);
                lines.push(detail_line);
            }
            
            if let Some(output) = &evt.output {
                let output_preview = shorten_text(output, 60);
                let result_line = Line::from(vec![
                    Span::raw("  "),
                    Span::styled("→ ", Style::default().fg(Color::Green)),
                    Span::styled(output_preview, Style::default().fg(Color::White)),
                ]);
                lines.push(result_line);
            }
        }
        
        lines.push(Line::from("")); // 事件间空行
    }
    
    // 应用滚动偏移
    let visible_lines = if app.active_panel == PanelKind::ToolEvents {
        lines.into_iter()
            .skip(app.scroll_offset)
            .collect()
    } else {
        lines
    };
    
    let paragraph = Paragraph::new(visible_lines)
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    
    // 高亮当前激活的面板
    let block = if is_active {
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default()
    };
    
    f.render_widget(paragraph.block(block), area);
}

// 辅助函数
fn format_timestamp(ts: &str) -> String {
    // 只显示时:分:秒
    ts.split('T')
        .nth(1)
        .and_then(|t| t.split('.').next())
        .unwrap_or(ts)
        .to_string()
}

fn format_args_preview(args: &serde_json::Value, max_len: usize) -> String {
    let s = args.to_string();
    if s.len() <= max_len {
        s
    } else {
        format!("{}...", &s[..max_len])
    }
}

fn shorten_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
    
    // 提示行
    let hint = match app.input_mode {
        InputMode::Normal => {
            "Normal Mode  ⌨ Press : for commands, / for search, Tab to switch"
# Normal Mode 快捷键
quit = ["q", "Q", "Ctrl+C"]
scroll_up = ["Up", "k"]
scroll_down = ["Down", "j"]
scroll_left = ["Left", "h"]
scroll_right = ["Right", "l"]
page_up = ["Ctrl+U", "PageUp"]
page_down = ["Ctrl+D", "PageDown"]
next_panel = ["Tab"]
prev_panel = ["Shift+Tab"]
pause = ["p"]
toggle_expand = ["Space"]
copy_line = ["y"]
copy_all = ["Y"]

# Mode 切换键
command_mode = [":"]
search_mode = ["/"]
filter_mode = ["?"]

# 搜索导航
search_next = ["n"]
search_prev = ["N
        InputMode::Filter => {
            "Filter Mode  Type pattern and press Enter (Esc to clear)"
        }
    };
    
    let hint_widget = Paragraph::new(hint)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Left);
    f.render_widget(hint_widget, lines[1]);
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

# 启动画面配置
show_splash = true
splash_duration_ms = 1500
splash_animation = true
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
- 模拟流式数据启动画面（ASCII Banner + 进度显示）
- [ ] 实现输入
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
