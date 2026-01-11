use std::collections::HashMap;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Visual progress monitor for task execution
///
/// Provides real-time progress bars for overall execution and individual tasks
pub struct ProgressMonitor {
    /// Multi-progress container
    multi: MultiProgress,
    /// Overall progress bar
    overall: ProgressBar,
    /// Per-task progress spinners
    task_bars: HashMap<String, ProgressBar>,
    /// Whether monitoring is enabled
    enabled: bool,
}

impl ProgressMonitor {
    /// Create a new progress monitor
    ///
    /// # Arguments
    ///
    /// * `total_tasks` - Total number of tasks to execute
    /// * `enabled` - Whether to enable visual progress (disabled for jsonl output)
    pub fn new(total_tasks: usize, enabled: bool) -> Self {
        if !enabled {
            return Self {
                multi: MultiProgress::new(),
                overall: ProgressBar::hidden(),
                task_bars: HashMap::new(),
                enabled: false,
            };
        }

        let multi = MultiProgress::new();
        let overall = multi.add(ProgressBar::new(total_tasks as u64));

        overall.set_style(
            ProgressStyle::default_bar()
                .template(
                    "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} tasks ({percent}%) {msg}",
                )
                .unwrap()
                .progress_chars("█▓▒░  "),
        );

        overall.set_message("Starting...");

        Self {
            multi,
            overall,
            task_bars: HashMap::new(),
            enabled: true,
        }
    }

    /// Add a task and create its progress spinner
    pub fn add_task(&mut self, task_id: &str) {
        if !self.enabled {
            return;
        }

        let bar = self.multi.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("  {spinner:.green} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        bar.set_message(format!("⏳ {}", task_id));
        bar.enable_steady_tick(Duration::from_millis(100));

        self.task_bars.insert(task_id.to_string(), bar);
    }

    /// Mark a task as completed
    pub fn complete_task(&mut self, task_id: &str, success: bool, duration_ms: u64) {
        if !self.enabled {
            return;
        }

        if let Some(bar) = self.task_bars.remove(task_id) {
            let icon = if success { "✅" } else { "❌" };
            bar.finish_with_message(format!("{} {} ({}ms)", icon, task_id, duration_ms));
        }

        self.overall.inc(1);
    }

    /// Update overall progress message
    pub fn set_message(&self, msg: &str) {
        if self.enabled {
            self.overall.set_message(msg.to_string());
        }
    }

    /// Mark stage progress
    pub fn update_stage(&self, stage_id: usize, total_stages: usize) {
        if self.enabled {
            self.overall
                .set_message(format!("Stage {}/{}", stage_id + 1, total_stages));
        }
    }

    /// Finish overall progress
    pub fn finish(&self, success: bool) {
        if !self.enabled {
            return;
        }

        let msg = if success {
            "✅ All tasks completed"
        } else {
            "❌ Execution failed"
        };

        self.overall.finish_with_message(msg.to_string());
    }

    /// Clear all progress indicators (cleanup)
    pub fn clear(&self) {
        if self.enabled {
            self.overall.finish_and_clear();
        }
    }
}

impl Drop for ProgressMonitor {
    fn drop(&mut self) {
        // Ensure all spinners are cleaned up
        for (_, bar) in self.task_bars.drain() {
            bar.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_monitor_disabled() {
        let mut monitor = ProgressMonitor::new(3, false);

        // Should not panic when disabled
        monitor.add_task("task1");
        monitor.complete_task("task1", true, 100);
        monitor.set_message("test");
        monitor.finish(true);
    }

    #[test]
    fn test_progress_monitor_enabled() {
        let mut monitor = ProgressMonitor::new(3, true);

        monitor.add_task("task1");
        monitor.add_task("task2");

        monitor.complete_task("task1", true, 100);
        monitor.complete_task("task2", false, 200);

        monitor.update_stage(0, 2);
        monitor.finish(true);
    }
}
