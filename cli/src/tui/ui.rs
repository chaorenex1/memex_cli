use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{InputMode, PanelKind, RawLine, RunStatus, TuiApp};

pub fn draw(f: &mut Frame<'_>, app: &TuiApp) {
    let size = f.area();
    if app.show_splash {
        draw_splash(f, size, app);
        return;
    }

    let input_height = if app.input_mode == InputMode::Prompt {
        5
    } else {
        2
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(input_height),
        ])
        .split(size);

    draw_header(f, chunks[0], app);
    draw_main(f, chunks[1], app);
    draw_input(f, chunks[2], app);
}

fn draw_header(f: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let run_id = if app.run_id.len() > 8 {
        &app.run_id[..8]
    } else {
        &app.run_id
    };
    let duration = format_duration(app.start.elapsed().as_secs());
    let tools = if app.tool_events_seen > 0 {
        app.tool_events_seen
    } else {
        app.tool_events.len()
    };
    let phase = if app.pending_qa && app.runtime_phase.is_none() {
        "qa".to_string()
    } else {
        app.runtime_phase
            .map(format_phase)
            .unwrap_or_else(|| "unknown".to_string())
    };
    let status_style = match app.status {
        RunStatus::Running => Style::default().fg(Color::Green),
        RunStatus::Paused => Style::default().fg(Color::Yellow),
        RunStatus::Completed(_) => Style::default().fg(Color::Cyan),
        RunStatus::Error(_) => Style::default().fg(Color::Red),
    };

    let mut line_parts = vec![
        Span::styled("Memex CLI", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  Run: "),
        Span::styled(run_id.to_string(), Style::default().fg(Color::Gray)),
        Span::raw("  Status: "),
        Span::styled(app.status_label(), status_style),
        Span::raw("  Phase: "),
        Span::styled(phase, Style::default().fg(Color::Gray)),
        Span::raw("  Tools: "),
        Span::styled(tools.to_string(), Style::default().fg(Color::Gray)),
        Span::raw("  Mem: "),
        Span::styled(
            app.memory_hits.to_string(),
            Style::default().fg(Color::Gray),
        ),
        Span::raw("  Dur: "),
        Span::styled(duration, Style::default().fg(Color::Gray)),
    ];
    if app.pending_qa {
        let qa_elapsed =
            format_duration(app.qa_started_at.unwrap_or(app.start).elapsed().as_secs());
        line_parts.push(Span::raw("  QA: "));
        line_parts.push(Span::styled(qa_elapsed, Style::default().fg(Color::Yellow)));
    }
    let line = Line::from(line_parts);

    let header = Paragraph::new(line).block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(header, area);
}

fn draw_main(f: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    draw_tool_events(f, chunks[0], app);
    draw_assistant_output(f, chunks[1], app);
    draw_raw_output(f, chunks[2], app);
}

fn draw_tool_events(f: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let active = app.active_panel == PanelKind::ToolEvents;
    let block = panel_block("Tool Events [1]", active);
    let lines = build_tool_event_lines(app);
    let offset = scroll_offset(lines.len(), area.height, app, PanelKind::ToolEvents);
    let widget = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((offset, 0));
    f.render_widget(widget, area);
}

fn draw_assistant_output(f: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let active = app.active_panel == PanelKind::AssistantOutput;
    let block = panel_block("Assistant Output [2]", active);
    let lines: Vec<Line> = app
        .assistant_lines
        .iter()
        .map(|line| Line::from(Span::raw(line.clone())))
        .collect();
    let offset = scroll_offset(lines.len(), area.height, app, PanelKind::AssistantOutput);
    let widget = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((offset, 0));
    f.render_widget(widget, area);
}

fn draw_raw_output(f: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let active = app.active_panel == PanelKind::RawOutput;
    let block = panel_block("Raw Output [3]", active);
    let lines: Vec<Line> = app.raw_lines.iter().map(raw_line_to_line).collect();
    let offset = scroll_offset(lines.len(), area.height, app, PanelKind::RawOutput);
    let widget = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((offset, 0));
    f.render_widget(widget, area);
}

fn draw_input(f: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let hint = match app.input_mode {
        InputMode::Prompt => {
            "Enter:run  Esc:clear  Ctrl+C/V/X:copy/paste/cut  Ctrl+D:quit".to_string()
        }
        InputMode::Normal => {
            // Show appropriate hint based on status
            match &app.status {
                RunStatus::Error(_) => {
                    "ERROR - Press 'n' or Enter for new query, 'q' or Ctrl+C to exit".to_string()
                }
                RunStatus::Completed(_) => {
                    "COMPLETED - Press 'n' or Enter for new query, 'q' or Ctrl+C to exit".to_string()
                }
                _ => {
                    if app.pending_qa {
                        let spinner = qa_spinner(app);
                        let qa_elapsed =
                            format_duration(app.qa_started_at.unwrap_or(app.start).elapsed().as_secs());
                        format!("QA loading... {} ({})", spinner, qa_elapsed)
                    } else {
                        "q:quit  Tab:next  1/2/3:panel  j/k:scroll  p:pause".to_string()
                    }
                }
            }
        }
    };
    draw_input_with_hint(f, area, app, hint);
}

fn draw_input_with_hint(f: &mut Frame<'_>, area: Rect, app: &TuiApp, hint: String) {
    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    let lines = if app.input_mode == InputMode::Prompt {
        build_prompt_lines(app, hint)
    } else {
        vec![Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::styled(hint, Style::default().fg(Color::Gray)),
        ])]
    };
    let input = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    f.render_widget(input, area);

    if app.input_mode == InputMode::Prompt {
        let (row, col) = prompt_cursor_pos(&app.input_buffer, app.input_cursor);
        let cursor_x = inner.x + 2 + col as u16;
        let cursor_y = inner.y + row as u16;
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn qa_spinner(app: &TuiApp) -> char {
    let frames = ['|', '/', '-', '\\'];
    let start = app.qa_started_at.unwrap_or(app.start);
    let elapsed = start.elapsed().as_millis() as usize;
    let idx = (elapsed / 120) % frames.len();
    frames[idx]
}

fn build_prompt_lines(app: &TuiApp, hint: String) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut first = true;
    
    // Get selection range if any
    let selection = if let (Some(start), Some(end)) = (app.selection_start, app.selection_end) {
        let (s, e) = if start <= end { (start, end) } else { (end, start) };
        if s < e { Some((s, e)) } else { None }
    } else {
        None
    };
    
    for raw in app.input_buffer.split('\n') {
        if first {
            let spans = if let Some((sel_start, sel_end)) = selection {
                build_line_with_selection(raw, 0, sel_start, sel_end, true)
            } else {
                vec![
                    Span::styled("> ", Style::default().fg(Color::Cyan)),
                    Span::raw(raw),
                ]
            };
            lines.push(Line::from(spans));
            first = false;
        } else {
            let line_start = lines.iter()
                .take(lines.len())
                .map(|_| app.input_buffer.split('\n').next().map(|s| s.len() + 1).unwrap_or(0))
                .sum::<usize>();
            
            let spans = if let Some((sel_start, sel_end)) = selection {
                let mut result = vec![Span::raw("  ")];
                result.extend(build_line_with_selection(raw, line_start, sel_start, sel_end, false));
                result
            } else {
                vec![Span::raw("  "), Span::raw(raw)]
            };
            lines.push(Line::from(spans));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::raw(""),
        ]));
    }
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::Gray),
    )));
    lines
}

fn build_line_with_selection(
    text: &str,
    line_start: usize,
    sel_start: usize,
    sel_end: usize,
    is_first_line: bool,
) -> Vec<Span<'_>> {
    let line_end = line_start + text.len();
    let mut spans = Vec::new();
    
    if is_first_line {
        spans.push(Span::styled("> ", Style::default().fg(Color::Cyan)));
    }
    
    // Before selection
    if sel_start > line_start {
        let before_end = sel_start.min(line_end);
        if before_end > line_start {
            let idx_start = 0;
            let idx_end = (before_end - line_start).min(text.len());
            spans.push(Span::raw(&text[idx_start..idx_end]));
        }
    }
    
    // Selection
    if sel_end > line_start && sel_start < line_end {
        let sel_local_start = sel_start.max(line_start) - line_start;
        let sel_local_end = sel_end.min(line_end) - line_start;
        if sel_local_end > sel_local_start {
            let selected_text = &text[sel_local_start.min(text.len())..sel_local_end.min(text.len())];
            spans.push(Span::styled(
                selected_text,
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }
    
    // After selection
    if sel_end < line_end {
        let after_start = sel_end.max(line_start) - line_start;
        if after_start < text.len() {
            spans.push(Span::raw(&text[after_start..]));
        }
    }
    
    spans
}

fn prompt_cursor_pos(input: &str, cursor: usize) -> (usize, usize) {
    let mut row = 0usize;
    let mut col = 0usize;
    let mut i = 0usize;
    for ch in input.chars() {
        if i >= cursor {
            break;
        }
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
        i += ch.len_utf8();
    }
    (row, col)
}

fn panel_block(title: &str, active: bool) -> Block<'_> {
    let mut block = Block::default().borders(Borders::ALL).title(title);
    if active {
        block = block.border_style(Style::default().fg(Color::Cyan));
    }
    block
}

fn build_tool_event_lines(app: &TuiApp) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    for (idx, ev) in app.tool_events.iter().enumerate() {
        let status = match ev.ok {
            Some(true) => ("OK", Color::Green),
            Some(false) => ("ERR", Color::Red),
            None => ("...", Color::Yellow),
        };
        let action = ev.action.clone().unwrap_or_else(|| "-".to_string());
        let header = Line::from(vec![
            Span::styled(
                format!("[{}] ", idx + 1),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(ev.ts.clone(), Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(status.0, Style::default().fg(status.1)),
            Span::raw(" "),
            Span::styled(ev.tool.clone(), Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            Span::styled(action, Style::default().fg(Color::Gray)),
        ]);
        lines.push(header);

        if app.expanded_events.contains(&idx) {
            if let Some(args) = &ev.args_preview {
                lines.push(Line::from(Span::styled(
                    format!("  args: {args}"),
                    Style::default().fg(Color::Gray),
                )));
            }
            if let Some(out) = &ev.output_preview {
                lines.push(Line::from(Span::styled(
                    format!("  out: {out}"),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
    }
    lines
}

fn raw_line_to_line(line: &RawLine) -> Line<'_> {
    let style = if line.is_stderr {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Gray)
    };
    Line::from(Span::styled(line.text.clone(), style))
}

fn scroll_offset(lines_len: usize, height: u16, app: &TuiApp, panel: PanelKind) -> u16 {
    if height == 0 {
        return 0;
    }
    let idx = match panel {
        PanelKind::ToolEvents => 0,
        PanelKind::AssistantOutput => 1,
        PanelKind::RawOutput => 2,
    };
    let max_offset = lines_len.saturating_sub(height as usize);
    let offset = if app.config.auto_scroll && !app.paused {
        max_offset
    } else {
        app.scroll_offsets[idx].min(max_offset)
    };
    offset as u16
}

fn draw_splash(f: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let block = Block::default().borders(Borders::ALL);
    f.render_widget(block, area);

    let banner = vec![
        "  __  __               ",
        " |  \\/  | ___ _ __ ___ ",
        " | |\\/| |/ _ \\ '_ ` _ \\",
        " | |  | |  __/ | | | | |",
        " |_|  |_|\\___|_| |_| |_|",
        "     Memex CLI",
        "",
    ];
    let mut lines: Vec<Line> = banner.into_iter().map(Line::from).collect();
    let init = if app.status_label() == "RUNNING" {
        "Initializing TUI..."
    } else {
        "Loading..."
    };
    lines.push(Line::from(init));
    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn format_duration(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m:02}:{s:02}")
}

fn format_phase(phase: memex_core::state::types::RuntimePhase) -> String {
    use memex_core::state::types::RuntimePhase;
    match phase {
        RuntimePhase::Idle => "idle",
        RuntimePhase::Initializing => "init",
        RuntimePhase::MemorySearch => "memory",
        RuntimePhase::RunnerStarting => "start",
        RuntimePhase::RunnerRunning => "run",
        RuntimePhase::ProcessingToolEvents => "tools",
        RuntimePhase::GatekeeperEvaluating => "gatekeeper",
        RuntimePhase::MemoryPersisting => "persist",
        RuntimePhase::Completed => "done",
        RuntimePhase::Failed => "fail",
    }
    .to_string()
}
