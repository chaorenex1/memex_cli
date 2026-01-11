use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use lazy_static::lazy_static;
use lru::LruCache;
use memex_core::executor::traits::{
    FileInfo, ProcessContext, ProcessMetadata, ProcessedTask, TaskProcessorPlugin,
};
use memex_core::executor::types::{ExecutableTask, FileProcessingConfig, ProcessorError};
use memmap2::Mmap;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

const DEFAULT_MAX_FILES: usize = 100;
const MAX_SINGLE_FILE_BYTES: u64 = 50 * 1024 * 1024;
const DEFAULT_MAX_TOTAL_SIZE_MB: u64 = 200;
const EMBED_SIZE_LIMIT: usize = 1024 * 1024;
const DEFAULT_CACHE_SIZE: usize = 100;

lazy_static! {
    static ref FILE_CACHE: Mutex<LruCache<PathBuf, Arc<Vec<u8>>>> = {
        Mutex::new(LruCache::new(NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap()))
    };
}

static CACHE_CAPACITY: AtomicUsize = AtomicUsize::new(DEFAULT_CACHE_SIZE);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilesMode {
    Embed,
    Ref,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilesEncoding {
    Utf8,
    Base64,
    Auto,
}

impl FilesMode {
    fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("auto").to_lowercase().as_str() {
            "embed" => FilesMode::Embed,
            "ref" => FilesMode::Ref,
            _ => FilesMode::Auto,
        }
    }
}

impl FilesEncoding {
    fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("auto").to_lowercase().as_str() {
            "utf8" | "utf-8" => FilesEncoding::Utf8,
            "base64" => FilesEncoding::Base64,
            _ => FilesEncoding::Auto,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedFile {
    display_path: String,
    mode: FilesMode,
    encoding: FilesEncoding,
    size: u64,
    modified: Option<std::time::SystemTime>,
    content: Option<ResolvedContent>,
}

#[derive(Debug, Clone)]
enum ResolvedContent {
    Text(String),
    Base64(String),
}

pub struct FileProcessorPlugin {
    config: FileProcessingConfig,
}

impl FileProcessorPlugin {
    pub fn new(config: FileProcessingConfig) -> Self {
        if config.enable_cache {
            configure_cache(config.cache_size);
        }
        Self { config }
    }

    async fn resolve_files_internal(
        &self,
        task: &ExecutableTask,
    ) -> Result<Vec<ResolvedFile>, ProcessorError> {
        let files = &task.metadata.files;
        if files.is_empty() {
            return Ok(Vec::new());
        }

        let workdir = task
            .metadata
            .workdir
            .clone()
            .unwrap_or_else(|| ".".to_string());
        let workdir = PathBuf::from(&workdir);
        let base_canon = tokio::fs::canonicalize(&workdir)
            .await
            .map_err(|_| {
                ProcessorError::InvalidInput(format!(
                    "working directory not found: {}",
                    workdir.display()
                ))
            })?;

        let files_mode = FilesMode::parse(task.metadata.files_mode.as_deref());
        let files_encoding = FilesEncoding::parse(task.metadata.files_encoding.as_deref());

        let max_files = if self.config.max_files == 0 {
            DEFAULT_MAX_FILES
        } else {
            self.config.max_files
        };
        let max_total_size = if self.config.max_total_size_mb == 0 {
            DEFAULT_MAX_TOTAL_SIZE_MB * 1024 * 1024
        } else {
            self.config.max_total_size_mb * 1024 * 1024
        };

        let semaphore = Arc::new(Semaphore::new(16));
        let seen = Arc::new(Mutex::new(HashSet::new()));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let config = Arc::new(self.config.clone());

        let mut futures = FuturesUnordered::new();
        let mut total_count = 0;

        for pattern in files {
            let pattern_path = workdir.join(pattern);
            let pattern_str = pattern_path.to_string_lossy().to_string();

            match glob::glob(&pattern_str) {
                Ok(paths) => {
                    for entry in paths {
                        match entry {
                            Ok(path) => {
                                total_count += 1;
                                if total_count > max_files {
                                    tracing::warn!(
                                        "File count exceeds limit ({}), stopping glob expansion",
                                        max_files
                                    );
                                    cancel_flag.store(true, Ordering::Relaxed);
                                    break;
                                }

                                let permit = semaphore.clone().acquire_owned().await.unwrap();
                                let base = base_canon.clone();
                                let mode = files_mode;
                                let encoding = files_encoding;
                                let cfg = config.clone();
                                let seen_clone = seen.clone();
                                let cancel_clone = cancel_flag.clone();

                                futures.push(tokio::spawn(async move {
                                    let result = process_single_file(
                                        path,
                                        base,
                                        mode,
                                        encoding,
                                        cfg,
                                        seen_clone,
                                        cancel_clone,
                                    )
                                    .await;
                                    drop(permit);
                                    result
                                }));
                            }
                            Err(e) => {
                                tracing::warn!("Glob error: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Invalid glob pattern '{}': {}", pattern, e);
                }
            }

            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }
        }

        let mut resolved = Vec::new();
        let mut total_size: u64 = 0;

        while let Some(result) = futures.next().await {
            match result {
                Ok(Ok(Some(file))) => {
                    total_size += file.size;
                    if total_size > max_total_size {
                        tracing::warn!(
                            "Total file size exceeds limit ({} > {} bytes), stopping",
                            total_size,
                            max_total_size
                        );
                        break;
                    }
                    resolved.push(file);
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                    tracing::warn!("File processing error: {}", e);
                }
                Err(e) => {
                    tracing::warn!("Task join error: {}", e);
                }
            }
        }

        resolved.sort_by(|a, b| a.display_path.cmp(&b.display_path));
        Ok(resolved)
    }

    fn compose_prompt_internal(&self, content: &str, files: &[ResolvedFile]) -> String {
        if files.is_empty() {
            return content.to_string();
        }

        let mut capacity = content.len() + 1024;
        for file in files {
            capacity += file.display_path.len() + 200;
            if let Some(file_content) = &file.content {
                capacity += match file_content {
                    ResolvedContent::Text(t) => t.len(),
                    ResolvedContent::Base64(b) => b.len(),
                };
            }
        }

        let mut prompt = String::with_capacity(capacity);

        for file in files {
            prompt.push_str("\n\n---FILE: ");
            prompt.push_str(&file.display_path);
            prompt.push_str("---\n");

            prompt.push_str(&format_file_metadata(file));
            prompt.push('\n');

            match (&file.mode, &file.content) {
                (FilesMode::Embed, Some(ResolvedContent::Text(text))) => {
                    if text.len() > EMBED_SIZE_LIMIT {
                        prompt.push_str(&format!(
                            "[Content truncated: {} bytes, showing first {} bytes]\n",
                            text.len(),
                            EMBED_SIZE_LIMIT
                        ));
                        prompt.push_str(&text[..EMBED_SIZE_LIMIT]);
                    } else {
                        prompt.push_str(text);
                    }
                }
                (FilesMode::Embed, Some(ResolvedContent::Base64(b64))) => {
                    prompt.push_str("[Binary content, base64 encoded]\n");
                    if b64.len() > EMBED_SIZE_LIMIT {
                        prompt.push_str(&format!(
                            "[Content truncated: {} bytes, showing first {} bytes]\n",
                            b64.len(),
                            EMBED_SIZE_LIMIT
                        ));
                        prompt.push_str(&b64[..EMBED_SIZE_LIMIT]);
                    } else {
                        prompt.push_str(b64);
                    }
                }
                (FilesMode::Ref, _) => {
                    prompt.push_str("[File reference only, content not embedded]\n");
                }
                (FilesMode::Auto, Some(content)) => {
                    let content_size = match content {
                        ResolvedContent::Text(t) => t.len(),
                        ResolvedContent::Base64(b) => b.len(),
                    };

                    if content_size > EMBED_SIZE_LIMIT {
                        prompt.push_str(&format!(
                            "[Auto mode: content too large ({} bytes), using ref mode]\n",
                            content_size
                        ));
                    } else {
                        match content {
                            ResolvedContent::Text(t) => prompt.push_str(t),
                            ResolvedContent::Base64(b) => {
                                prompt.push_str("[Binary content, base64 encoded]\n");
                                prompt.push_str(b);
                            }
                        }
                    }
                }
                _ => {}
            }

            prompt.push_str("\n---END FILE---\n");
        }

        prompt.push_str("\n\n");
        prompt.push_str(content);
        prompt
    }
}

#[async_trait]
impl TaskProcessorPlugin for FileProcessorPlugin {
    fn name(&self) -> &str {
        "file-processor"
    }

    fn priority(&self) -> i32 {
        100
    }

    async fn process(
        &self,
        task: &ExecutableTask,
        _context: &ProcessContext,
    ) -> Result<ProcessedTask, ProcessorError> {
        let files = self.resolve_files_internal(task).await?;
        let enhanced = self.compose_prompt_internal(&task.content, &files);

        let metadata = ProcessMetadata {
            files: files
                .iter()
                .map(|file| FileInfo {
                    path: file.display_path.clone(),
                    size: file.size,
                })
                .collect(),
            ..ProcessMetadata::default()
        };

        Ok(ProcessedTask {
            original: task.clone(),
            enhanced_content: enhanced,
            metadata,
        })
    }
}

fn configure_cache(capacity: usize) {
    if capacity == 0 {
        return;
    }

    let current = CACHE_CAPACITY.load(Ordering::Relaxed);
    if current == capacity {
        return;
    }

    if let Ok(mut cache) = FILE_CACHE.lock() {
        *cache = LruCache::new(NonZeroUsize::new(capacity).unwrap());
        CACHE_CAPACITY.store(capacity, Ordering::Relaxed);
    }
}

async fn read_file_with_mmap(
    path: &Path,
    config: &FileProcessingConfig,
    file_size_bytes: u64,
) -> Result<Option<Vec<u8>>, ProcessorError> {
    if !config.enable_mmap {
        return Ok(None);
    }

    let size_mb = file_size_bytes / (1024 * 1024);
    if size_mb < config.mmap_threshold_mb {
        return Ok(None);
    }

    let path_owned = path.to_path_buf();
    let data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, ProcessorError> {
        let file = std::fs::File::open(&path_owned)
            .map_err(|e| ProcessorError::Io(format!("open {}: {}", path_owned.display(), e)))?;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| ProcessorError::Other(format!("mmap failed: {}", e)))?;

        Ok(mmap.to_vec())
    })
    .await
    .map_err(|e| ProcessorError::Other(format!("mmap task failed: {}", e)))??;

    Ok(Some(data))
}

async fn read_file_cached(
    path: &Path,
    config: &FileProcessingConfig,
    file_size_bytes: u64,
) -> Result<Vec<u8>, ProcessorError> {
    if config.enable_cache {
        let path_buf = path.to_path_buf();
        if let Ok(mut cache) = FILE_CACHE.lock() {
            if let Some(content) = cache.get(&path_buf) {
                return Ok((**content).clone());
            }
        }
    }

    let bytes = if let Some(data) = read_file_with_mmap(path, config, file_size_bytes).await? {
        data
    } else {
        tokio::fs::read(path)
            .await
            .map_err(|e| ProcessorError::Io(format!("read {}: {}", path.display(), e)))?
    };

    if config.enable_cache {
        let path_buf = path.to_path_buf();
        let arc_bytes = Arc::new(bytes.clone());
        if let Ok(mut cache) = FILE_CACHE.lock() {
            cache.put(path_buf, arc_bytes);
        }
    }

    Ok(bytes)
}

async fn process_single_file(
    path: PathBuf,
    base_canon: PathBuf,
    files_mode: FilesMode,
    files_encoding: FilesEncoding,
    config: Arc<FileProcessingConfig>,
    seen: Arc<Mutex<HashSet<PathBuf>>>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<Option<ResolvedFile>, ProcessorError> {
    if cancel_flag.load(Ordering::Relaxed) {
        return Ok(None);
    }

    let canon = tokio::fs::canonicalize(&path)
        .await
        .map_err(|_| ProcessorError::Io(format!("file not found: {}", path.display())))?;

    {
        let mut s = seen.lock().unwrap();
        if s.contains(&canon) {
            return Ok(None);
        }
        s.insert(canon.clone());
    }

    let display_path = if let Ok(rel) = canon.strip_prefix(&base_canon) {
        rel.display().to_string()
    } else {
        canon.display().to_string()
    };

    let meta = tokio::fs::metadata(&canon)
        .await
        .map_err(|e| ProcessorError::Io(format!("metadata {}: {}", canon.display(), e)))?;

    if !meta.is_file() {
        return Ok(None);
    }

    let file_size = meta.len();
    let modified = meta.modified().ok();

    if file_size > MAX_SINGLE_FILE_BYTES {
        tracing::warn!(
            "File {} exceeds size limit ({} > {} bytes), skipping",
            display_path,
            file_size,
            MAX_SINGLE_FILE_BYTES
        );
        return Ok(None);
    }

    let content = if files_mode == FilesMode::Ref {
        None
    } else {
        let bytes = read_file_cached(&canon, &config, file_size).await?;

        let resolved = match files_encoding {
            FilesEncoding::Utf8 => match String::from_utf8(bytes.clone()) {
                Ok(text) => ResolvedContent::Text(text),
                Err(_) => {
                    tracing::warn!(
                        "File {} is not valid UTF-8, using base64",
                        display_path
                    );
                    ResolvedContent::Base64(base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &bytes,
                    ))
                }
            },
            FilesEncoding::Base64 => ResolvedContent::Base64(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &bytes,
            )),
            FilesEncoding::Auto => match String::from_utf8(bytes.clone()) {
                Ok(text) => ResolvedContent::Text(text),
                Err(_) => ResolvedContent::Base64(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &bytes,
                )),
            },
        };

        Some(resolved)
    };

    Ok(Some(ResolvedFile {
        display_path,
        mode: files_mode,
        encoding: files_encoding,
        size: file_size,
        modified,
        content,
    }))
}

fn format_file_metadata(file: &ResolvedFile) -> String {
    let mut meta = format!("<!-- size: {} bytes", file.size);

    if let Some(modified) = file.modified {
        if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
            meta.push_str(&format!(", modified: {}", duration.as_secs()));
        }
    }

    meta.push_str(&format!(", encoding: {:?}", file.encoding));
    meta.push_str(" -->");

    meta
}
