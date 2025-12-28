//! # 状态管理模块
//!
//! 负责管理 memex-cli 运行过程中的全局状态、会话状态和事件状态。
//!
//! ## 设计原则
//!
//! 1. **线程安全**：使用 Arc<RwLock<T>> 实现多线程共享
//! 2. **状态分层**：应用状态、会话状态、运行时状态分离
//! 3. **事件驱动**：状态变更触发事件通知
//! 4. **可观测**：所有状态变更都可追踪
//! 5. **故障恢复**：支持状态快照和恢复

pub mod manager;
pub mod session;
pub mod snapshot;
pub mod transitions;
pub mod types;

pub use manager::{StateManager, StateManagerHandle};
pub use session::{SessionState, SessionStatus};
pub use snapshot::{SnapshotManager, StateSnapshot};
pub use transitions::{StateTransition, TransitionError};
pub use types::{AppState, RuntimeState, StateEvent};
