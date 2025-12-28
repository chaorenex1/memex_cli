//! 状态快照和恢复

use super::session::SessionState;
use super::types::AppState;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 状态快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// 快照 ID
    pub snapshot_id: String,
    /// 快照时间
    pub timestamp: DateTime<Utc>,
    /// 应用状态
    pub app_state: AppState,
    /// 所有会话
    pub sessions: HashMap<String, SessionState>,
    /// 快照版本
    pub version: String,
}

impl StateSnapshot {
    /// 创建新快照
    pub fn new(app_state: AppState, sessions: HashMap<String, SessionState>) -> Self {
        Self {
            snapshot_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            app_state,
            sessions,
            version: "1.0.0".to_string(),
        }
    }

    /// 序列化为 JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize snapshot")
    }

    /// 从 JSON 反序列化
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize snapshot")
    }

    /// 保存到文件
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = self.to_json()?;
        fs::write(path.as_ref(), json)
            .with_context(|| format!("Failed to write snapshot to {:?}", path.as_ref()))
    }

    /// 从文件加载
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read snapshot from {:?}", path.as_ref()))?;
        Self::from_json(&json)
    }
}

/// 快照管理器
pub struct SnapshotManager {
    /// 快照存储目录
    snapshot_dir: PathBuf,
    /// 最大保留快照数
    max_snapshots: usize,
}

impl SnapshotManager {
    /// 创建快照管理器
    pub fn new<P: Into<PathBuf>>(snapshot_dir: P, max_snapshots: usize) -> Result<Self> {
        let snapshot_dir = snapshot_dir.into();

        // 确保目录存在
        if !snapshot_dir.exists() {
            fs::create_dir_all(&snapshot_dir).with_context(|| {
                format!("Failed to create snapshot directory: {:?}", snapshot_dir)
            })?;
        }

        Ok(Self {
            snapshot_dir,
            max_snapshots,
        })
    }

    /// 保存快照
    pub fn save_snapshot(&self, snapshot: &StateSnapshot) -> Result<PathBuf> {
        let filename = format!("snapshot_{}.json", snapshot.snapshot_id);
        let path = self.snapshot_dir.join(filename);

        snapshot.save_to_file(&path)?;

        // 清理旧快照
        self.cleanup_old_snapshots()?;

        Ok(path)
    }

    /// 加载最新快照
    pub fn load_latest_snapshot(&self) -> Result<Option<StateSnapshot>> {
        let snapshots = self.list_snapshots()?;

        if snapshots.is_empty() {
            return Ok(None);
        }

        let latest_path = &snapshots[0];
        let snapshot = StateSnapshot::load_from_file(latest_path)?;
        Ok(Some(snapshot))
    }

    /// 加载指定快照
    pub fn load_snapshot_by_id(&self, snapshot_id: &str) -> Result<StateSnapshot> {
        let filename = format!("snapshot_{}.json", snapshot_id);
        let path = self.snapshot_dir.join(filename);
        StateSnapshot::load_from_file(path)
    }

    /// 列出所有快照（按时间倒序）
    pub fn list_snapshots(&self) -> Result<Vec<PathBuf>> {
        let mut snapshots = Vec::new();

        for entry in fs::read_dir(&self.snapshot_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                    if filename.starts_with("snapshot_") {
                        snapshots.push(path);
                    }
                }
            }
        }

        // 按修改时间倒序排序
        snapshots.sort_by(|a, b| {
            let a_time = fs::metadata(a).and_then(|m| m.modified()).ok();
            let b_time = fs::metadata(b).and_then(|m| m.modified()).ok();
            b_time.cmp(&a_time)
        });

        Ok(snapshots)
    }

    /// 清理旧快照
    fn cleanup_old_snapshots(&self) -> Result<()> {
        let snapshots = self.list_snapshots()?;

        if snapshots.len() > self.max_snapshots {
            for path in snapshots.iter().skip(self.max_snapshots) {
                fs::remove_file(path)
                    .with_context(|| format!("Failed to remove old snapshot: {:?}", path))?;
            }
        }

        Ok(())
    }

    /// 删除所有快照
    pub fn clear_snapshots(&self) -> Result<usize> {
        let snapshots = self.list_snapshots()?;
        let count = snapshots.len();

        for path in snapshots {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove snapshot: {:?}", path))?;
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_snapshot_serialization() {
        let snapshot = StateSnapshot::new(AppState::default(), HashMap::new());

        let json = snapshot.to_json().unwrap();
        let restored = StateSnapshot::from_json(&json).unwrap();

        assert_eq!(snapshot.snapshot_id, restored.snapshot_id);
        assert_eq!(snapshot.version, restored.version);
    }

    #[test]
    fn test_snapshot_manager() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SnapshotManager::new(temp_dir.path(), 5).unwrap();

        let snapshot = StateSnapshot::new(AppState::default(), HashMap::new());

        let path = manager.save_snapshot(&snapshot).unwrap();
        assert!(path.exists());

        let loaded = manager.load_latest_snapshot().unwrap();
        assert!(loaded.is_some());

        let loaded_snapshot = loaded.unwrap();
        assert_eq!(snapshot.snapshot_id, loaded_snapshot.snapshot_id);
    }

    #[test]
    fn test_snapshot_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SnapshotManager::new(temp_dir.path(), 3).unwrap();

        // 创建 5 个快照
        for _ in 0..5 {
            let snapshot = StateSnapshot::new(AppState::default(), HashMap::new());
            manager.save_snapshot(&snapshot).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // 应该只保留 3 个
        let snapshots = manager.list_snapshots().unwrap();
        assert_eq!(snapshots.len(), 3);
    }
}
