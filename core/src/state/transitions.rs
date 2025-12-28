//! 状态转换规则和验证

use super::types::RuntimePhase;
use thiserror::Error;

/// 状态转换错误
#[derive(Debug, Error)]
pub enum TransitionError {
    #[error("Invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: RuntimePhase,
        to: RuntimePhase,
    },
    #[error("Cannot transition from terminal state {state:?}")]
    FromTerminalState { state: RuntimePhase },
}

/// 状态转换
pub struct StateTransition;

impl StateTransition {
    /// 验证状态转换是否合法
    pub fn validate(from: RuntimePhase, to: RuntimePhase) -> Result<(), TransitionError> {
        // 终态不能转换
        if matches!(from, RuntimePhase::Completed | RuntimePhase::Failed) {
            return Err(TransitionError::FromTerminalState { state: from });
        }

        // 定义合法的转换
        let is_valid = match (from, to) {
            // 从 Idle 可以转到初始化
            (RuntimePhase::Idle, RuntimePhase::Initializing) => true,

            // 从初始化可以转到记忆检索
            (RuntimePhase::Initializing, RuntimePhase::MemorySearch) => true,

            // 从记忆检索可以转到 Runner 启动
            (RuntimePhase::MemorySearch, RuntimePhase::RunnerStarting) => true,

            // 从 Runner 启动可以转到运行中
            (RuntimePhase::RunnerStarting, RuntimePhase::RunnerRunning) => true,

            // 从运行中可以转到处理工具事件
            (RuntimePhase::RunnerRunning, RuntimePhase::ProcessingToolEvents) => true,

            // 从处理工具事件可以转到 Gatekeeper 评估
            (RuntimePhase::ProcessingToolEvents, RuntimePhase::GatekeeperEvaluating) => true,

            // 从 Gatekeeper 可以转到记忆沉淀
            (RuntimePhase::GatekeeperEvaluating, RuntimePhase::MemoryPersisting) => true,

            // 从记忆沉淀可以转到完成
            (RuntimePhase::MemoryPersisting, RuntimePhase::Completed) => true,

            // 各阶段都可以转到 Completed 或 Failed
            (_, RuntimePhase::Completed) | (_, RuntimePhase::Failed) => true,

            // 运行中的阶段可以互相转换（处理并发情况）
            (RuntimePhase::ProcessingToolEvents, RuntimePhase::RunnerRunning) => true,

            // 其他转换都不合法
            _ => false,
        };

        if is_valid {
            Ok(())
        } else {
            Err(TransitionError::InvalidTransition { from, to })
        }
    }

    /// 获取下一个建议的阶段
    pub fn next_phase(current: RuntimePhase) -> Option<RuntimePhase> {
        match current {
            RuntimePhase::Idle => Some(RuntimePhase::Initializing),
            RuntimePhase::Initializing => Some(RuntimePhase::MemorySearch),
            RuntimePhase::MemorySearch => Some(RuntimePhase::RunnerStarting),
            RuntimePhase::RunnerStarting => Some(RuntimePhase::RunnerRunning),
            RuntimePhase::RunnerRunning => Some(RuntimePhase::ProcessingToolEvents),
            RuntimePhase::ProcessingToolEvents => Some(RuntimePhase::GatekeeperEvaluating),
            RuntimePhase::GatekeeperEvaluating => Some(RuntimePhase::MemoryPersisting),
            RuntimePhase::MemoryPersisting => Some(RuntimePhase::Completed),
            RuntimePhase::Completed | RuntimePhase::Failed => None,
        }
    }

    /// 判断是否为终态
    pub fn is_terminal(phase: RuntimePhase) -> bool {
        matches!(phase, RuntimePhase::Completed | RuntimePhase::Failed)
    }

    /// 获取阶段的可读描述
    pub fn phase_description(phase: RuntimePhase) -> &'static str {
        match phase {
            RuntimePhase::Idle => "空闲状态",
            RuntimePhase::Initializing => "初始化中",
            RuntimePhase::MemorySearch => "记忆检索中",
            RuntimePhase::RunnerStarting => "启动 Runner",
            RuntimePhase::RunnerRunning => "Runner 运行中",
            RuntimePhase::ProcessingToolEvents => "处理工具事件",
            RuntimePhase::GatekeeperEvaluating => "Gatekeeper 评估中",
            RuntimePhase::MemoryPersisting => "记忆沉淀中",
            RuntimePhase::Completed => "已完成",
            RuntimePhase::Failed => "已失败",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        assert!(StateTransition::validate(RuntimePhase::Idle, RuntimePhase::Initializing).is_ok());

        assert!(StateTransition::validate(
            RuntimePhase::RunnerRunning,
            RuntimePhase::ProcessingToolEvents
        )
        .is_ok());

        assert!(
            StateTransition::validate(RuntimePhase::MemoryPersisting, RuntimePhase::Completed)
                .is_ok()
        );
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(
            StateTransition::validate(RuntimePhase::Idle, RuntimePhase::RunnerRunning).is_err()
        );

        assert!(StateTransition::validate(RuntimePhase::Completed, RuntimePhase::Idle).is_err());
    }

    #[test]
    fn test_terminal_states() {
        assert!(StateTransition::is_terminal(RuntimePhase::Completed));
        assert!(StateTransition::is_terminal(RuntimePhase::Failed));
        assert!(!StateTransition::is_terminal(RuntimePhase::RunnerRunning));
    }

    #[test]
    fn test_next_phase() {
        assert_eq!(
            StateTransition::next_phase(RuntimePhase::Idle),
            Some(RuntimePhase::Initializing)
        );
        assert_eq!(StateTransition::next_phase(RuntimePhase::Completed), None);
    }
}
