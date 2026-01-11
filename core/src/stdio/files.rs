#![deprecated(note = "Use executor FileProcessorPlugin instead.")]
//! 文件处理模块：负责STDIO任务的文件解析、读取和prompt组装

use crate::api::StdioTask;
use crate::config::StdioConfig;
use crate::error::stdio::StdioError;
use crate::stdio::types::{FilesEncoding, FilesMode};
use futures::stream::{FuturesUnordered, StreamExt};
use lazy_static::lazy_static;
use lru::LruCache;
use memmap2::Mmap;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

/// 最大文件数限制（可通过环境变量覆盖）
const MAX_FILES: usize = 100;

/// 单个文件大小限制（字节）
const MAX_SINGLE_FILE: u64 = 50 * 1024 * 1024; // 50MB

/// 所有文件总大小限制（字节）
const MAX_TOTAL_SIZE: u64 = 200 * 1024 * 1024; // 200MB

/// 嵌入模式下内容大小阈值（超过则截断或切换为ref模式）
const EMBED_SIZE_LIMIT: usize = 1024 * 1024; // 1MB

/// 已解析文件结构
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    pub display_path: String,
    pub mode: FilesMode,
    pub encoding: FilesEncoding,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
    pub content: Option<ResolvedContent>,
}

/// 文件内容（文本或Base64）
#[derive(Debug, Clone)]
pub enum ResolvedContent {
    Text(String),
    Base64(String),
}

// 全局LRU文件缓存（Level 3.3优化）
lazy_static! {
    static ref FILE_CACHE: Mutex<LruCache<PathBuf, Arc<Vec<u8>>>> = {
        let capacity = std::env::var("MEM_STDIO_FILE_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);
        Mutex::new(LruCache::new(NonZeroUsize::new(capacity).unwrap()))
    };
}

/// 使用mmap读取大文件（Level 3.1优化）
async fn read_file_with_mmap(
    path: &Path,
    threshold_mb: u64,
    file_size_bytes: u64,
) -> Result<Option<Vec<u8>>, StdioError> {
    let size_mb = file_size_bytes / (1024 * 1024);
    if size_mb < threshold_mb {
        return Ok(None);
    }

    let path_owned = path.to_path_buf();
    let data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, StdioError> {
        let file = std::fs::File::open(&path_owned).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StdioError::FileNotFound(path_owned.display().to_string())
            } else {
                StdioError::FileAccessDenied(path_owned.display().to_string())
            }
        })?;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| StdioError::BackendError(format!("mmap failed: {}", e)))?;

        Ok(mmap.to_vec())
    })
    .await
    .map_err(|e: tokio::task::JoinError| StdioError::BackendError(e.to_string()))??;

    Ok(Some(data))
}

/// 带缓存的文件读取（Level 3.3优化）
async fn read_file_cached(
    path: &Path,
    threshold_mb: u64,
    file_size_bytes: u64,
    enable_cache: bool,
) -> Result<Vec<u8>, StdioError> {
    // 检查缓存
    if enable_cache {
        let path_buf = path.to_path_buf();
        if let Ok(mut cache) = FILE_CACHE.lock() {
            if let Some(content) = cache.get(&path_buf) {
                return Ok((**content).clone());
            }
        }
    }

    // 读取文件（优先mmap）
    let bytes = if let Some(data) = read_file_with_mmap(path, threshold_mb, file_size_bytes).await?
    {
        data
    } else {
        tokio::fs::read(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StdioError::FileNotFound(path.display().to_string())
            } else {
                StdioError::FileAccessDenied(path.display().to_string())
            }
        })?
    };

    // 写入缓存
    if enable_cache {
        let path_buf = path.to_path_buf();
        let arc_bytes = Arc::new(bytes.clone());
        if let Ok(mut cache) = FILE_CACHE.lock() {
            cache.put(path_buf, arc_bytes);
        }
    }

    Ok(bytes)
}

/// 处理单个文件：规范化路径、读取内容、处理编码
async fn process_single_file(
    path: PathBuf,
    base_canon: PathBuf,
    files_mode: FilesMode,
    files_encoding: FilesEncoding,
    stdio_config: Arc<StdioConfig>,
    seen: Arc<Mutex<HashSet<PathBuf>>>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<Option<ResolvedFile>, StdioError> {
    // 检查取消标志
    if cancel_flag.load(Ordering::Relaxed) {
        return Ok(None);
    }

    // 规范化路径
    let canon = tokio::fs::canonicalize(&path).await.map_err(|_| {
        StdioError::FileNotFound(path.display().to_string())
    })?;

    // 去重检查
    {
        let mut s = seen.lock().unwrap();
        if s.contains(&canon) {
            return Ok(None);
        }
        s.insert(canon.clone());
    }

    // 计算相对路径显示
    let display_path = if let Ok(rel) = canon.strip_prefix(&base_canon) {
        rel.display().to_string()
    } else {
        canon.display().to_string()
    };

    // 获取元数据
    let meta = tokio::fs::metadata(&canon).await.map_err(|_| {
        StdioError::FileAccessDenied(canon.display().to_string())
    })?;

    if !meta.is_file() {
        return Ok(None);
    }

    let file_size = meta.len();
    let modified = meta.modified().ok();

    // 检查文件大小
    if file_size > MAX_SINGLE_FILE {
        tracing::warn!(
            "File {} exceeds size limit ({} > {} bytes), skipping",
            display_path,
            file_size,
            MAX_SINGLE_FILE
        );
        return Ok(None);
    }

    // 读取文件内容（如果需要）
    let content = if files_mode == FilesMode::Ref {
        None
    } else {
        let bytes = read_file_cached(
            &canon,
            stdio_config.mmap_threshold_mb,
            file_size,
            stdio_config.enable_file_cache,
        )
        .await?;

        // 根据编码处理
        let resolved = match files_encoding {
            FilesEncoding::Utf8 => {
                match String::from_utf8(bytes.clone()) {
                    Ok(text) => ResolvedContent::Text(text),
                    Err(_) => {
                        // UTF-8解析失败，回退到Base64
                        tracing::warn!("File {} is not valid UTF-8, using base64", display_path);
                        ResolvedContent::Base64(base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            &bytes,
                        ))
                    }
                }
            }
            FilesEncoding::Base64 => ResolvedContent::Base64(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &bytes,
            )),
            FilesEncoding::Auto => {
                // Auto模式：尝试UTF-8，失败则Base64
                match String::from_utf8(bytes.clone()) {
                    Ok(text) => ResolvedContent::Text(text),
                    Err(_) => ResolvedContent::Base64(base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &bytes,
                    )),
                }
            }
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

/// 解析任务中的文件引用，返回已处理的文件列表
pub async fn resolve_files(
    task: &StdioTask,
    stdio_config: &StdioConfig,
) -> Result<Vec<ResolvedFile>, StdioError> {
    let files = &task.files;
    if files.is_empty() {
        return Ok(Vec::new());
    }

    // 获取工作目录规范路径
    let workdir = PathBuf::from(&task.workdir);
    let base_canon = tokio::fs::canonicalize(&workdir)
        .await
        .map_err(|_| StdioError::InvalidPath(format!("working directory not found: {}", task.workdir)))?;

    // 并发控制（最多16个文件同时处理）
    let semaphore = Arc::new(Semaphore::new(16));
    let seen = Arc::new(Mutex::new(HashSet::new()));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let files_mode = task.files_mode;
    let files_encoding = task.files_encoding;

    let stdio_config = Arc::new(stdio_config.clone());

    // Glob展开 + 并行处理
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
                            if total_count > MAX_FILES {
                                tracing::warn!(
                                    "File count exceeds limit ({}), stopping glob expansion",
                                    MAX_FILES
                                );
                                cancel_flag.store(true, Ordering::Relaxed);
                                break;
                            }

                            let permit = semaphore.clone().acquire_owned().await.unwrap();
                            let base = base_canon.clone();
                            let mode = files_mode;
                            let encoding = files_encoding;
                            let config = stdio_config.clone();
                            let seen_clone = seen.clone();
                            let cancel_clone = cancel_flag.clone();

                            futures.push(tokio::spawn(async move {
                                let result = process_single_file(
                                    path,
                                    base,
                                    mode,
                                    encoding,
                                    config,
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

    // 收集结果
    let mut resolved = Vec::new();
    let mut total_size: u64 = 0;

    while let Some(result) = futures.next().await {
        match result {
            Ok(Ok(Some(file))) => {
                total_size += file.size;
                if total_size > MAX_TOTAL_SIZE {
                    tracing::warn!(
                        "Total file size exceeds limit ({} > {} bytes), stopping",
                        total_size,
                        MAX_TOTAL_SIZE
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

    // 按路径排序（确保输出稳定）
    resolved.sort_by(|a, b| a.display_path.cmp(&b.display_path));

    Ok(resolved)
}

/// 组装增强prompt：将文件内容嵌入到任务content中
pub fn compose_prompt(task: &StdioTask, files: &[ResolvedFile]) -> String {
    if files.is_empty() {
        return task.content.clone();
    }

    // 预计算容量（优化内存分配）
    let mut capacity = task.content.len() + 1024; // 基础容量
    for file in files {
        capacity += file.display_path.len() + 200; // 元数据和标记
        if let Some(content) = &file.content {
            capacity += match content {
                ResolvedContent::Text(t) => t.len(),
                ResolvedContent::Base64(b) => b.len(),
            };
        }
    }

    let mut prompt = String::with_capacity(capacity);

    // 添加文件内容
    for file in files {
        prompt.push_str("\n\n---FILE: ");
        prompt.push_str(&file.display_path);
        prompt.push_str("---\n");

        // 元数据
        prompt.push_str(&format_file_metadata(file));
        prompt.push('\n');

        // 内容
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
                // Auto模式：根据大小自动选择
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

    // 原始用户prompt
    prompt.push_str("\n\n");
    prompt.push_str(&task.content);

    prompt
}

/// 格式化文件元数据为HTML注释
pub fn format_file_metadata(file: &ResolvedFile) -> String {
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
