use std::time::Duration;

use memex_core::error::RunnerError;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use super::events::{InputReader, InputEvent};
use super::ui;
use super::{InputMode, PromptAction, TuiApp};

pub async fn prompt_in_tui(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut TuiApp,
) -> Result<String, RunnerError> {
    app.set_prompt_mode();
    let (input_reader, mut input_rx) = InputReader::start();
    let mut tick =
        tokio::time::interval(Duration::from_millis(app.config.update_interval_ms.max(16)));

    let input_area = terminal.get_frame().area();
    
    loop {
        tokio::select! {
            Some(event) = input_rx.recv() => {
                match event {
                    InputEvent::Key(key) => {
                        match app.handle_prompt_key(key) {
                            PromptAction::Submit => break,
                            PromptAction::Clear => {
                                app.input_buffer.clear();
                                app.input_cursor = 0;
                                app.clear_selection();
                            }
                            PromptAction::Exit => {
                                input_reader.stop();
                                return Err(RunnerError::Spawn("input cancelled".to_string()));
                            }
                            PromptAction::None => {}
                        }
                    }
                    InputEvent::Mouse(mouse) => {
                        app.handle_mouse(mouse, input_area);
                    }
                }
            }
            _ = tick.tick() => {}
        }

        terminal
            .draw(|f| ui::draw(f, app))
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    }

    input_reader.stop();
    app.input_mode = InputMode::Normal;
    Ok(app.input_buffer.clone())
}
