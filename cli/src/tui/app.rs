use std::collections::{HashSet, VecDeque};
use std::time::Instant;

use crossterm::event::KeyEvent;
use memex_core::config::TuiConfig;
use memex_core::state::types::RuntimePhase;
use memex_core::tool_event::ToolEvent;
use memex_core::tui::TuiEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    ToolEvents,
    AssistantOutput,
    RawOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Prompt,
}

#[derive(Debug, Clone)]
pub enum RunStatus {
    Running,
    Paused,
    Completed(i32),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAction {
    None,
    Submit,
    Cancel,
    Exit,
}
#[derive(Debug, Clone)]
pub struct RawLine {
    pub text: String,
    pub is_stderr: bool,
}

#[derive(Debug, Clone)]
pub struct ToolEventEntry {
    pub ts: String,
    pub tool: String,
    pub action: Option<String>,
    pub ok: Option<bool>,
    pub args_preview: Option<String>,
    pub output_preview: Option<String>,
}

pub struct TuiApp {
    pub config: TuiConfig,
    pub start: Instant,
    pub run_id: String,
    pub status: RunStatus,
    pub runtime_phase: Option<RuntimePhase>,
    pub memory_hits: usize,
    pub tool_events_seen: usize,
    pub pending_qa: bool,
    pub qa_started_at: Option<Instant>,
    pub active_panel: PanelKind,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub input_cursor: usize,
    pub paused: bool,
    pub tool_events: VecDeque<ToolEventEntry>,
    pub assistant_lines: VecDeque<String>,
    pub raw_lines: VecDeque<RawLine>,
    pub expanded_events: HashSet<usize>,
    pub scroll_offsets: [usize; 3],
    pub show_splash: bool,
    pub splash_start: Instant,
}

impl TuiApp {
    pub fn new(config: TuiConfig, run_id: String) -> Self {
        let now = Instant::now();
        Self {
            config,
            start: now,
            run_id,
            status: RunStatus::Running,
            runtime_phase: None,
            memory_hits: 0,
            tool_events_seen: 0,
            pending_qa: false,
            qa_started_at: None,
            active_panel: PanelKind::ToolEvents,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            input_cursor: 0,
            paused: false,
            tool_events: VecDeque::new(),
            assistant_lines: VecDeque::new(),
            raw_lines: VecDeque::new(),
            expanded_events: HashSet::new(),
            scroll_offsets: [0; 3],
            show_splash: true,
            splash_start: now,
        }
    }

    pub fn status_label(&self) -> String {
        match self.status {
            RunStatus::Running => {
                if self.paused {
                    "PAUSED".to_string()
                } else {
                    "RUNNING".to_string()
                }
            }
            RunStatus::Completed(code) => format!("DONE({code})"),
            RunStatus::Error(ref msg) => format!("ERROR({})", truncate(msg, 24)),
            RunStatus::Paused => "PAUSED".to_string(),
        }
    }

    pub fn maybe_hide_splash(&mut self) {
        if !self.config.show_splash {
            self.show_splash = false;
            return;
        }
        let elapsed = self.splash_start.elapsed().as_millis() as u64;
        if elapsed >= self.config.splash_duration_ms {
            self.show_splash = false;
        }
    }

    pub fn handle_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::ToolEvent(ev) => self.push_tool_event(*ev),
            TuiEvent::AssistantOutput(line) => self.push_assistant_line(line),
            TuiEvent::RawStdout(line) => self.push_raw_line(line, false),
            TuiEvent::RawStderr(line) => self.push_raw_line(line, true),
            TuiEvent::StatusUpdate { .. } => {}
            TuiEvent::StateUpdate {
                phase,
                memory_hits,
                tool_events,
            } => {
                self.runtime_phase = Some(phase);
                self.memory_hits = memory_hits;
                self.tool_events_seen = tool_events;
                self.pending_qa = false;
                self.qa_started_at = None;
            }
            TuiEvent::RunComplete { exit_code } => {
                self.status = RunStatus::Completed(exit_code);
                self.pending_qa = false;
                self.qa_started_at = None;
            }
            TuiEvent::Error(msg) => {
                self.status = RunStatus::Error(msg);
                self.pending_qa = false;
                self.qa_started_at = None;
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyModifiers;

        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Tab => self.next_panel(),
            KeyCode::Char('1') => self.active_panel = PanelKind::ToolEvents,
            KeyCode::Char('2') => self.active_panel = PanelKind::AssistantOutput,
            KeyCode::Char('3') => self.active_panel = PanelKind::RawOutput,
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(1),
            KeyCode::PageUp | KeyCode::Char('u')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.scroll_up(10);
            }
            KeyCode::PageDown | KeyCode::Char('d')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.scroll_down(10);
            }
            KeyCode::Char('g') => self.scroll_to_top(),
            KeyCode::Char('G') => self.scroll_to_bottom(),
            KeyCode::Char('p') => self.toggle_pause(),
            KeyCode::Char(' ') => self.toggle_expand_last(),
            _ => {}
        }
        false
    }

    pub fn is_done(&self) -> bool {
        matches!(self.status, RunStatus::Completed(_) | RunStatus::Error(_))
    }

    pub fn set_prompt_mode(&mut self) {
        self.input_mode = InputMode::Prompt;
        self.input_cursor = self.input_buffer.len();
        self.show_splash = false;
    }

    pub fn handle_prompt_key(&mut self, key: KeyEvent) -> PromptAction {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyModifiers;

        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.insert_char('\n');
                    PromptAction::None
                } else {
                    PromptAction::Submit
                }
            }
            KeyCode::Esc => PromptAction::Cancel,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                PromptAction::Exit
            }
            KeyCode::Backspace => {
                self.backspace();
                PromptAction::None
            }
            KeyCode::Left => {
                self.move_left();
                PromptAction::None
            }
            KeyCode::Right => {
                self.move_right();
                PromptAction::None
            }
            KeyCode::Home => {
                self.input_cursor = 0;
                PromptAction::None
            }
            KeyCode::End => {
                self.input_cursor = self.input_buffer.len();
                PromptAction::None
            }
            KeyCode::Char(ch) => {
                self.insert_char(ch);
                PromptAction::None
            }
            _ => PromptAction::None,
        }
    }

    fn insert_char(&mut self, ch: char) {
        self.input_buffer.insert(self.input_cursor, ch);
        self.input_cursor += ch.len_utf8();
    }

    fn backspace(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let prev = prev_char_boundary(&self.input_buffer, self.input_cursor);
        if prev < self.input_cursor {
            self.input_buffer.replace_range(prev..self.input_cursor, "");
            self.input_cursor = prev;
        }
    }

    fn move_left(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        self.input_cursor = prev_char_boundary(&self.input_buffer, self.input_cursor);
    }

    fn move_right(&mut self) {
        if self.input_cursor >= self.input_buffer.len() {
            return;
        }
        let next = next_char_boundary(&self.input_buffer, self.input_cursor);
        self.input_cursor = next;
    }

    fn push_tool_event(&mut self, ev: ToolEvent) {
        let ts = format_timestamp(ev.ts.as_deref());
        let tool = ev.tool.unwrap_or_else(|| "unknown".to_string());
        let args_preview = format_json_preview(&ev.args, 80);
        let output_preview = ev
            .output
            .as_ref()
            .map(|v| format_json_preview(v, 80))
            .unwrap_or(None);
        let entry = ToolEventEntry {
            ts,
            tool,
            action: ev.action,
            ok: ev.ok,
            args_preview,
            output_preview,
        };
        self.tool_events.push_back(entry);
        trim_vec_deque(&mut self.tool_events, self.config.max_tool_events);
    }

    fn push_assistant_line(&mut self, line: String) {
        if line.is_empty() {
            return;
        }
        self.assistant_lines.push_back(line);
        trim_vec_deque(&mut self.assistant_lines, self.config.max_output_lines);
    }

    fn push_raw_line(&mut self, line: String, is_stderr: bool) {
        if line.is_empty() {
            return;
        }
        self.raw_lines.push_back(RawLine {
            text: line,
            is_stderr,
        });
        trim_vec_deque(&mut self.raw_lines, self.config.max_output_lines);
    }

    fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            PanelKind::ToolEvents => PanelKind::AssistantOutput,
            PanelKind::AssistantOutput => PanelKind::RawOutput,
            PanelKind::RawOutput => PanelKind::ToolEvents,
        };
    }

    fn scroll_up(&mut self, amount: usize) {
        let idx = panel_index(self.active_panel);
        self.scroll_offsets[idx] = self.scroll_offsets[idx].saturating_sub(amount);
        self.paused = true;
    }

    fn scroll_down(&mut self, amount: usize) {
        let idx = panel_index(self.active_panel);
        self.scroll_offsets[idx] = self.scroll_offsets[idx].saturating_add(amount);
        self.paused = true;
    }

    fn scroll_to_top(&mut self) {
        let idx = panel_index(self.active_panel);
        self.scroll_offsets[idx] = 0;
        self.paused = true;
    }

    fn scroll_to_bottom(&mut self) {
        let idx = panel_index(self.active_panel);
        self.scroll_offsets[idx] = usize::MAX / 2;
        self.paused = false;
    }

    fn toggle_pause(&mut self) {
        if matches!(self.status, RunStatus::Completed(_) | RunStatus::Error(_)) {
            return;
        }
        self.paused = !self.paused;
        if self.paused {
            self.status = RunStatus::Paused;
        } else {
            self.status = RunStatus::Running;
        }
    }

    fn toggle_expand_last(&mut self) {
        if self.tool_events.is_empty() {
            return;
        }
        let idx = self.tool_events.len().saturating_sub(1);
        if !self.expanded_events.insert(idx) {
            self.expanded_events.remove(&idx);
        }
    }
}

fn panel_index(panel: PanelKind) -> usize {
    match panel {
        PanelKind::ToolEvents => 0,
        PanelKind::AssistantOutput => 1,
        PanelKind::RawOutput => 2,
    }
}

fn format_timestamp(ts: Option<&str>) -> String {
    let Some(ts) = ts else {
        return "--:--:--".to_string();
    };
    let time = ts
        .split('T')
        .nth(1)
        .and_then(|t| t.split('.').next())
        .unwrap_or(ts);
    time.to_string()
}

fn format_json_preview(value: &serde_json::Value, max_len: usize) -> Option<String> {
    let s = value.to_string();
    if s == "null" {
        return None;
    }
    Some(truncate(&s, max_len))
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let mut out = s[..max_len].to_string();
    out.push_str("...");
    out
}

fn trim_vec_deque<T>(deque: &mut VecDeque<T>, max_len: usize) {
    while deque.len() > max_len {
        deque.pop_front();
    }
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let mut prev = 0;
    for (i, _) in s.char_indices() {
        if i >= idx {
            break;
        }
        prev = i;
    }
    prev
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut iter = s[idx..].char_indices();
    let Some((_, ch)) = iter.next() else {
        return s.len();
    };
    idx + ch.len_utf8()
}
