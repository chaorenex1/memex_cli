//! 状态管理系统使用示例
//!
//! 演示如何在 memex-cli 中集成和使用状态管理

use anyhow::Result;
use memex_core::state::types::RuntimePhase;
use memex_core::state::{StateEvent, StateManager};

/// 示例：完整的会话生命周期
#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 1. 创建状态管理器
    let manager = StateManager::new();
    let handle = manager.handle();

    // 2. 启动事件监听器（后台任务）
    let mut event_rx = manager.subscribe();
    tokio::spawn(async move {
        println!("📡 Event listener started\n");
        while let Ok(event) = event_rx.recv().await {
            match event {
                StateEvent::SessionCreated { session_id, .. } => {
                    println!("✓ Session created: {}", session_id);
                }
                StateEvent::SessionStateChanged {
                    session_id,
                    new_phase,
                    ..
                } => {
                    println!("→ Session {} → {:?}", session_id, new_phase);
                }
                StateEvent::ToolEventReceived {
                    session_id,
                    event_count,
                    ..
                } => {
                    println!(
                        "🔧 Session {} received {} tool events",
                        session_id, event_count
                    );
                }
                StateEvent::MemoryHit {
                    session_id,
                    hit_count,
                    ..
                } => {
                    println!("💾 Session {} memory hits: {}", session_id, hit_count);
                }
                StateEvent::SessionCompleted {
                    session_id,
                    exit_code,
                    duration_ms,
                    ..
                } => {
                    println!(
                        "✓ Session {} completed (exit={}, duration={}ms)",
                        session_id, exit_code, duration_ms
                    );
                }
                StateEvent::SessionFailed {
                    session_id, error, ..
                } => {
                    println!("✗ Session {} failed: {}", session_id, error);
                }
                _ => {}
            }
        }
    });

    // 给事件监听器一点启动时间
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("🚀 Starting memex-cli session\n");

    // 3. 创建会话
    let session_id = handle
        .create_session(Some("example-run-123".to_string()))
        .await?;

    // 4. 模拟会话生命周期

    // 初始化阶段
    println!("\n[Phase 1] Initializing...");
    handle
        .transition_phase(&session_id, RuntimePhase::Initializing)
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // 记忆检索阶段
    println!("[Phase 2] Memory search...");
    handle
        .transition_phase(&session_id, RuntimePhase::MemorySearch)
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // 模拟记忆命中
    manager
        .update_session(&session_id, |session| {
            session.increment_memory_hits(3);
        })
        .await?;

    // Runner 启动阶段
    println!("[Phase 3] Starting runner...");
    handle
        .transition_phase(&session_id, RuntimePhase::RunnerStarting)
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Runner 运行阶段
    println!("[Phase 4] Runner running...");
    handle
        .transition_phase(&session_id, RuntimePhase::RunnerRunning)
        .await?;

    // 模拟设置 Runner PID
    manager
        .update_session(&session_id, |session| {
            session.set_runner_pid(12345);
        })
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 处理工具事件阶段
    println!("[Phase 5] Processing tool events...");
    handle
        .transition_phase(&session_id, RuntimePhase::ProcessingToolEvents)
        .await?;

    // 模拟接收工具事件
    for i in 1..=5 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        manager
            .update_session(&session_id, |session| {
                session.increment_tool_events(i);
            })
            .await?;
    }

    // Gatekeeper 评估阶段
    println!("[Phase 6] Gatekeeper evaluating...");
    handle
        .transition_phase(&session_id, RuntimePhase::GatekeeperEvaluating)
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // 模拟 Gatekeeper 决策
    manager
        .update_session(&session_id, |session| {
            session.set_gatekeeper_decision(memex_core::state::types::GatekeeperDecisionSnapshot {
                should_write_candidate: true,
                reasons: vec!["High quality response".to_string()],
                signals: std::collections::HashMap::new(),
            });
        })
        .await?;

    // 记忆沉淀阶段
    println!("[Phase 7] Memory persisting...");
    handle
        .transition_phase(&session_id, RuntimePhase::MemoryPersisting)
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // 完成会话
    println!("[Phase 8] Completing session...");
    handle.complete(&session_id, 0).await?;

    // 5. 查询最终状态
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    println!("\n📊 Final Statistics:");
    let app_state = manager.get_app_state().await;
    println!("   Active sessions: {}", app_state.active_sessions);
    println!("   Completed sessions: {}", app_state.completed_sessions);

    let session = manager.get_session(&session_id).await?;
    println!("\n📈 Session Details:");
    println!("   Session ID: {}", session.session_id);
    println!("   Duration: {}ms", session.duration_ms());
    println!("   Tool events: {}", session.runtime.tool_events_count);
    println!("   Memory hits: {}", session.runtime.memory_hits);
    println!("   Final phase: {:?}", session.runtime.phase);

    let stats = manager.get_session_stats().await;
    println!("\n📊 Overall Stats:");
    println!("   Running: {}", stats.running);
    println!("   Completed: {}", stats.completed);
    println!("   Failed: {}", stats.failed);

    println!("\n✅ Example completed successfully");

    Ok(())
}
