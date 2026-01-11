use serde::{Deserialize, Serialize};

/// Execution options for the current executor engine (legacy path).
#[derive(Debug, Clone)]
pub struct ExecutionOpts {
    /// Output stream format: "text" or "jsonl"
    pub stream_format: String,

    /// Bytes to capture from each task output
    pub capture_bytes: usize,

    /// Verbose output (include timestamps and metadata)
    pub verbose: bool,

    /// Quiet mode (suppress non-essential output)
    pub quiet: bool,

    /// ASCII-only markers (no Unicode)
    pub ascii: bool,

    /// Maximum parallel tasks (overrides config if Some)
    pub max_parallel: Option<usize>,

    /// Resume run ID (for resuming interrupted runs)
    pub resume_run_id: Option<String>,

    /// Resume context (injected into first task)
    pub resume_context: Option<String>,

    /// Enable visual progress bar (disabled for jsonl output)
    pub progress_bar: bool,

    // STDIO优化配置（从StdioConfig扩展）
    /// Enable event buffering to reduce syscalls (Level 2.1)
    pub enable_event_buffering: bool,

    /// Event buffer size
    pub event_buffer_size: usize,

    /// Event flush interval in milliseconds
    pub event_flush_interval_ms: u64,

    /// Enable adaptive concurrency based on CPU usage (Level 2.2)
    pub enable_adaptive_concurrency: bool,

    /// Enable LRU file cache (Level 3.3)
    pub enable_file_cache: bool,

    /// Enable memory-mapped I/O for large files (Level 3.1)
    pub enable_mmap_large_files: bool,

    /// Memory-mapped I/O threshold in MB
    pub mmap_threshold_mb: u64,
}

impl ExecutionOpts {
    /// Convert from StdioRunOpts for backward compatibility (uses default STDIO optimization flags)
    pub fn from_stdio_opts(opts: &crate::stdio::StdioRunOpts) -> Self {
        // Enable progress bar only for text output (not jsonl) and when not quiet
        let progress_bar = opts.stream_format == "text" && !opts.quiet;

        Self {
            stream_format: opts.stream_format.clone(),
            capture_bytes: opts.capture_bytes,
            verbose: opts.verbose,
            quiet: opts.quiet,
            ascii: opts.ascii,
            max_parallel: None,
            resume_run_id: opts.resume_run_id.clone(),
            resume_context: opts.resume_context.clone(),
            progress_bar,
            // Default STDIO optimization flags
            enable_event_buffering: true,
            event_buffer_size: 100,
            event_flush_interval_ms: 100,
            enable_adaptive_concurrency: true,
            enable_file_cache: true,
            enable_mmap_large_files: true,
            mmap_threshold_mb: 10,
        }
    }

    /// Convert from StdioRunOpts + StdioConfig for full STDIO optimization support
    pub fn from_stdio_config(
        opts: &crate::stdio::StdioRunOpts,
        stdio_config: &crate::config::StdioConfig,
    ) -> Self {
        // Enable progress bar only for text output (not jsonl) and when not quiet
        let progress_bar = opts.stream_format == "text" && !opts.quiet;

        Self {
            stream_format: opts.stream_format.clone(),
            capture_bytes: opts.capture_bytes,
            verbose: opts.verbose,
            quiet: opts.quiet,
            ascii: opts.ascii,
            max_parallel: None,
            resume_run_id: opts.resume_run_id.clone(),
            resume_context: opts.resume_context.clone(),
            progress_bar,
            // STDIO优化配置（从StdioConfig读取）
            enable_event_buffering: stdio_config.enable_event_buffering,
            event_buffer_size: stdio_config.event_buffer_size,
            event_flush_interval_ms: stdio_config.event_flush_interval_ms,
            enable_adaptive_concurrency: stdio_config.enable_adaptive_concurrency,
            enable_file_cache: stdio_config.enable_file_cache,
            enable_mmap_large_files: stdio_config.enable_mmap_large_files,
            mmap_threshold_mb: stdio_config.mmap_threshold_mb,
        }
    }
}

/// Executor plugin configuration (new path).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionConfig {
    #[serde(default)]
    pub file_processing: FileProcessingConfig,

    #[serde(default)]
    pub output: OutputConfig,

    #[serde(default)]
    pub retry: RetryConfig,

    #[serde(default)]
    pub concurrency: ConcurrencyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProcessingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub enable_mmap: bool,
    #[serde(default)]
    pub mmap_threshold_mb: u64,
    #[serde(default)]
    pub enable_cache: bool,
    #[serde(default)]
    pub cache_size: usize,
    #[serde(default)]
    pub max_files: usize,
    #[serde(default)]
    pub max_total_size_mb: u64,
}

impl Default for FileProcessingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_mmap: true,
            mmap_threshold_mb: 10,
            enable_cache: true,
            cache_size: 100,
            max_files: 100,
            max_total_size_mb: 200,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_output_format")]
    pub format: String,
    #[serde(default)]
    pub pretty_print: bool,
    #[serde(default)]
    pub ascii_only: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: default_output_format(),
            pretty_print: false,
            ascii_only: false,
        }
    }
}

fn default_output_format() -> String {
    "jsonl".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub base_delay_ms: u64,
    #[serde(default)]
    pub max_delay_ms: u64,
    #[serde(default)]
    pub max_attempts: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            strategy: default_retry_strategy(),
            base_delay_ms: 100,
            max_delay_ms: 5000,
            max_attempts: 3,
        }
    }
}

fn default_retry_strategy() -> String {
    "exponential-backoff".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    #[serde(default = "default_concurrency_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub min_concurrency: usize,
    #[serde(default)]
    pub max_concurrency: usize,
    #[serde(default)]
    pub base_concurrency: usize,
    #[serde(default)]
    pub cpu_threshold_low: f32,
    #[serde(default)]
    pub cpu_threshold_high: f32,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            strategy: default_concurrency_strategy(),
            min_concurrency: 2,
            max_concurrency: 32,
            base_concurrency: 8,
            cpu_threshold_low: 50.0,
            cpu_threshold_high: 80.0,
        }
    }
}

fn default_concurrency_strategy() -> String {
    "adaptive".to_string()
}
