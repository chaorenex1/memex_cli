# Memory 生命周期文档

**目的**: 详细说明 Memory 系统中 Candidate/Hit/Validation 三种记录类型的生命周期、状态转换和决策逻辑

**相关文档**:
- [Architecture Analysis](ARCHITECTURE_ANALYSIS.md) - 架构问题分析
- [Memory Architecture](MEMORY_ARCHITECTURE.md) - Memory 分层设计

---

## 目录

1. [概述](#1-概述)
2. [数据模型](#2-数据模型)
3. [完整生命周期](#3-完整生命周期)
4. [状态转换图](#4-状态转换图)
5. [关键决策逻辑](#5-关键决策逻辑)
6. [典型场景](#6-典型场景)
7. [常见问题](#7-常见问题)

---

## 1. 概述

### 1.1 三种记录类型

Memory 系统中有三种独立的记录类型，各自有不同的用途和生命周期：

| 记录类型 | 用途 | 触发时机 | 数据来源 |
|---------|-----|---------|---------|
| **Candidate** | 新答案候选 | Post-run | 本次执行的输出（stdout、tool_events） |
| **Hit** | 命中统计 | Post-run | 已注入 prompt 的历史答案 |
| **Validation** | 验证结果 | Post-run | 被使用的历史答案在本次执行中的表现 |

### 1.2 生命周期阶段

```
Pre-Run (阶段 1)         Execute (阶段 2)         Post-Run (阶段 3)         Storage (阶段 4)
─────────────────        ────────────────        ─────────────────        ────────────────
Memory.search()    →     Backend 执行      →     Gatekeeper 评估   →     Memory Service
                                                  ├─ record_hit()
                                                  ├─ record_validation()
                                                  └─ record_candidate()
```

---

## 2. 数据模型

### 2.1 Candidate（候选答案）

```rust
pub struct QACandidatePayload {
    pub project_id: String,        // 项目标识符
    pub query: String,              // 用户问题
    pub answer: String,             // AI 生成的答案
    pub context: String,            // 上下文（stdout + stderr + tool_events）
    pub tags: Vec<String>,          // 标签（用于分类）
    pub confidence: f32,            // 置信度 (0.0-1.0)
}
```

**字段说明**:
- `query`: 原始用户输入（未 inject 前的 query）
- `answer`: 从 stdout 或 tool_events 提取的核心答案文本
- `context`: 完整上下文，包含执行过程细节
- `confidence`: 答案质量评估（基于工具使用数量、输出长度等）

### 2.2 Validation（验证结果）

```rust
pub struct QAValidationPayload {
    pub project_id: String,
    pub qa_id: String,              // 关联的 QA 记录 ID
    pub result: ValidationResult,   // 验证结果
    pub notes: Option<String>,      // 验证备注
}

pub enum ValidationResult {
    Pass,      // 执行成功，答案有效
    Fail,      // 执行失败，答案可能有问题
    Partial,   // 部分成功（部分工具失败）
}
```

**字段说明**:
- `qa_id`: 指向被验证的历史 QA 记录
- `result`: 基于 exit_code 和 tool_events 的评估结果
- `notes`: 包含 exit_code、duration、stdout/stderr 摘要等

### 2.3 Hit（命中记录）

```rust
pub struct QAHitsPayload {
    pub project_id: String,
    pub references: Vec<QAReferencePayload>,  // 多个 QA 引用
}

pub struct QAReferencePayload {
    pub qa_id: String,
    pub shown: Option<bool>,    // 是否展示给用户（注入到 prompt）
    pub used: Option<bool>,     // 是否被 AI 实际使用（从 stdout 提取）
}
```

**字段说明**:
- `shown`: 该 QA 是否被 Gatekeeper 选中注入到 prompt
- `used`: AI 输出中是否引用了该 QA（通过 stdout 中的 qa_id 检测）
- 单次执行可有多个 references（多个历史答案被展示/使用）

---

## 3. 完整生命周期

### 阶段 1: Pre-Run（搜索记忆）

```
User Query
    ↓
Memory.search(QASearchPayload {
    project_id: "my-project",
    query: "如何实现用户认证？",
    limit: 10,
    min_score: 0.7
})
    ↓
SearchMatch[] ← Memory Service 返回历史 QA 记录
    [
        { qa_id: "qa-123", score: 0.95, validation_level: 2, trust: 0.88, ... },
        { qa_id: "qa-456", score: 0.82, validation_level: 1, trust: 0.75, ... },
        ...
    ]
    ↓
Gatekeeper.prepare_inject(matches)
    筛选逻辑:
    - 过滤: status not in active_statuses → 拒绝
    - 过滤: freshness < 0.001 (太旧) → 拒绝
    - 过滤: consecutive_fail >= threshold → 拒绝
    - 排序: (validation_level, trust, score, freshness)
    - 选择: 前 N 个（max_inject）
    ↓
InjectItem[] ← 筛选后的高质量记录
    [
        { qa_id: "qa-123", question: "...", answer: "...", context: "..." },
        { qa_id: "qa-456", question: "...", answer: "...", context: "..." },
    ]
    ↓
Merged Prompt = User Query + InjectItem[]
    用户问题：如何实现用户认证？

    参考答案 1 (QA-123):
    问题：用户认证最佳实践
    答案：使用 JWT + OAuth2，示例代码...

    参考答案 2 (QA-456):
    问题：认证中间件实现
    答案：创建认证中间件，示例代码...
```

**记录的 shown_qa_ids**: `["qa-123", "qa-456"]`

---

### 阶段 2: Execute（执行）

```
Backend (Claude/Codex/Gemini) 执行 Merged Prompt
    ↓
输出:
    - stdout: "我推荐使用 JWT 认证。参考 QA-123 的方案，修改如下..."
    - stderr: ""
    - tool_events: [
        { tool: "write_file", path: "auth.py", ... },
        { tool: "bash", command: "pytest tests/test_auth.py", ... },
    ]
    - exit_code: 0
    ↓
RunOutcome {
    exit_code: 0,
    tool_events: [...],
    stdout_tail: "推荐使用 JWT 认证。参考 QA-123...",
    stderr_tail: "",
    shown_qa_ids: ["qa-123", "qa-456"],  // 从 InjectItem 提取
    used_qa_ids: ["qa-123"],             // 从 stdout 提取（检测到 "QA-123"）
}
```

---

### 阶段 3: Post-Run（质量评估与记忆写入）

```
Gatekeeper.evaluate(matches, outcome, tool_events)
    ↓
GatekeeperDecision {
    hit_refs: [
        { qa_id: "qa-123", shown: true, used: true },
        { qa_id: "qa-456", shown: true, used: false },
    ],
    validate_plans: [
        { qa_id: "qa-123", result: Pass, signal_strength: "strong", ... },
    ],
    should_write_candidate: true,
}
    ↓
    ├─→ 3.1 Record Hit（命中记录）
    │   ────────────────────────────────────────────
    │   Memory.record_hit(QAHitsPayload {
    │       project_id: "my-project",
    │       references: [
    │           { qa_id: "qa-123", shown: true, used: true },
    │           { qa_id: "qa-456", shown: true, used: false },
    │       ]
    │   })
    │
    │   用途:
    │   - 追踪历史答案的展示和使用情况
    │   - 计算答案的使用率 (use_count / hit_count)
    │   - 影响未来排序（高使用率答案优先）
    │
    ├─→ 3.2 Record Validation（验证结果）
    │   ────────────────────────────────────────────
    │   Memory.record_validation(QAValidationPayload {
    │       project_id: "my-project",
    │       qa_id: "qa-123",
    │       result: Pass,  // exit_code == 0
    │       notes: "exit_code=0, duration_ms=1250, used=true, ..."
    │   })
    │
    │   用途:
    │   - 记录历史答案在本次执行中的表现
    │   - 更新答案的 validation_level (0-3)
    │   - 更新 trust 分数和 consecutive_fail 计数
    │
    │   触发条件:
    │   - 仅对 used_qa_ids 中的答案记录验证
    │   - 若无 used_qa_ids，对第一个 inject 的答案记录验证
    │
    │   验证结果判定:
    │   - exit_code == 0 → Pass
    │   - exit_code != 0 且有部分成功工具 → Partial
    │   - exit_code != 0 且无成功工具 → Fail
    │
    └─→ 3.3 Record Candidate（候选答案）
        ────────────────────────────────────────────
        candidates = extract_candidates(
            query: "如何实现用户认证？",
            stdout: "推荐使用 JWT 认证...",
            stderr: "",
            tool_events: [...]
        )
        ↓
        for c in candidates {
            Memory.record_candidate(QACandidatePayload {
                project_id: "my-project",
                query: "如何实现用户认证？",
                answer: "推荐使用 JWT 认证。创建 auth.py...",
                context: "stdout: ...\ntool_events: write_file, bash...",
                confidence: 0.85,
                tags: ["authentication", "jwt", "python"],
            })
        }

        用途:
        - 将本次执行的输出作为新的答案候选
        - 供未来相似查询检索使用
        - 候选可通过 Validation 升级为 Verified

        触发条件（所有条件需同时满足）:
        - exit_code == 0 (执行成功)
        - tool_events 包含实质性内容 (非空操作)
        - 输出符合质量标准 (confidence >= threshold)
        - 无安全风险（未检测到敏感信息）
        - top1_score < skip_if_top1_score_ge (已有高分答案时不写入)
        - !has_strong (已有强验证答案时不写入)
```

---

### 阶段 4: Storage（持久化）

```
Memory Service (外部 HTTP 服务)
    ↓
Database (PostgreSQL/SQLite/...)
    ↓
QA Records 更新:

1. qa-123 (被使用且验证通过):
   - hit_count: 45 → 46
   - use_count: 32 → 33
   - validation_level: 1 → 2 (pass 次数增加)
   - consecutive_fail: 0 (保持)
   - trust: 0.88 → 0.90 (上升)

2. qa-456 (被展示但未使用):
   - hit_count: 28 → 29
   - use_count: 15 (不变)
   - validation_level: 1 (不变)
   - consecutive_fail: 0 (保持)

3. 新 Candidate (qa-789):
   - qa_id: "qa-789" (新生成)
   - query: "如何实现用户认证？"
   - answer: "推荐使用 JWT 认证..."
   - validation_level: 0 (candidate)
   - trust: 0.50 (初始)
   - consecutive_fail: 0
```

---

## 4. 状态转换图

### 4.1 Validation Level 升级路径

```
Candidate (Level 0)
    ↓
    │ Validation: pass (首次)
    ↓
Verified (Level 1)
    ↓
    │ Validation: pass (多次，总计 >= 3 次)
    ↓
Confirmed (Level 2)
    ↓
    │ Validation: pass (高频使用，use_count >= 10)
    ↓
Gold Standard (Level 3)
```

### 4.2 降级和阻止机制

```
任意 Level:
    │ Validation: fail
    ↓
consecutive_fail += 1
trust -= penalty (例如 -0.1)

if consecutive_fail >= block_threshold (例如 3):
    Status: blocked (不再被 prepare_inject 选中)
    ↓
    需要手动干预或连续 pass 才能恢复
```

### 4.3 Trust 分数动态调整

```
Validation: pass
    trust += 0.05 (上限 1.0)
    consecutive_fail = 0

Validation: partial
    trust += 0.02
    consecutive_fail += 0.5

Validation: fail
    trust -= 0.10 (下限 0.0)
    consecutive_fail += 1
```

---

## 5. 关键决策逻辑

### 5.1 何时写入 Candidate？

**决策点**: `core/src/gatekeeper/evaluate.rs`

```rust
let should_write_candidate =
    run.exit_code == 0                    // 执行成功
    && !tool_events.is_empty()            // 有实质性操作
    && quality_signals.confidence >= 0.45 // 置信度阈值
    && !has_secrets                       // 无敏感信息
    && !is_trivial_output                 // 非琐碎输出
    && !has_strong                        // 无强验证答案
    && top1_score < cfg.skip_if_top1_score_ge  // 已有答案分数不过高
```

**提取逻辑**: `core/src/memory/candidates.rs`

```rust
pub fn extract_candidates(
    config: &CandidateExtractConfig,
    query: &str,
    stdout: &str,
    stderr: &str,
    tool_events: &[ToolEventLite],
) -> Vec<CandidateDraft> {
    // 1. 从 tool_events 提取核心操作
    let steps = extract_tool_steps(tool_events, config);

    // 2. 从 stdout 提取答案文本
    let answer = extract_answer_text(stdout, config);

    // 3. 构建上下文（stdout + stderr + tool_steps）
    let context = build_context(stdout, stderr, &steps);

    // 4. 检测敏感信息（redact=true 时）
    if config.redact && contains_secrets(&answer, &context) {
        if config.strict_secret_block {
            return vec![];  // 严格模式：直接拒绝
        } else {
            // 宽松模式：脱敏处理
            answer = redact_secrets(answer);
            context = redact_secrets(context);
        }
    }

    // 5. 评估置信度
    let confidence = assess_confidence(&answer, &steps);

    if confidence < config.min_confidence {
        return vec![];
    }

    vec![CandidateDraft {
        query: query.to_string(),
        answer,
        context,
        confidence,
        tags: extract_tags(&steps),
    }]
}
```

**置信度评估**:
```rust
fn assess_confidence(answer: &str, steps: &[String]) -> f32 {
    let mut score = 0.5;  // 基准分

    // 工具使用数量（越多越可信）
    if steps.len() >= 3 {
        score += 0.2;
    } else if steps.len() >= 1 {
        score += 0.1;
    }

    // 答案长度（适中最好）
    if answer.len() >= 50 && answer.len() <= 1000 {
        score += 0.15;
    } else if answer.len() >= 20 {
        score += 0.05;
    }

    // 代码块检测（有代码示例更可信）
    if answer.contains("```") {
        score += 0.1;
    }

    score.min(1.0)
}
```

---

### 5.2 何时记录 Validation？

**决策点**: `core/src/gatekeeper/evaluate.rs`

```rust
let validate_targets: Vec<String> = if !run.used_qa_ids.is_empty() {
    // 优先验证被使用的答案
    run.used_qa_ids.clone()
} else if let Some(first) = inject_list.first() {
    // 若无 used，验证第一个注入的答案
    vec![first.qa_id.clone()]
} else {
    // 无答案注入，不验证
    vec![]
};

let validate_plans: Vec<ValidatePlan> = validate_targets
    .iter()
    .map(|qa_id| {
        let result = if run.exit_code == 0 {
            ValidationResult::Pass
        } else if partial_success_detected(&run.tool_events) {
            ValidationResult::Partial
        } else {
            ValidationResult::Fail
        };

        ValidatePlan {
            qa_id: qa_id.clone(),
            result,
            signal_strength: assess_signal_strength(&run),
            strong_signal: run.exit_code == 0 && !run.tool_events.is_empty(),
            context: Some(build_validation_notes(&run)),
            payload: serde_json::json!({
                "exit_code": run.exit_code,
                "duration_ms": run.duration_ms,
                "stdout_tail_digest": digest(&run.stdout_tail),
                "stderr_tail_digest": digest(&run.stderr_tail),
                // ...
            }),
        }
    })
    .collect();
```

**部分成功检测**:
```rust
fn partial_success_detected(tool_events: &[ToolEvent]) -> bool {
    let total = tool_events.len();
    let failed = tool_events.iter().filter(|e| e.exit_code != Some(0)).count();

    // 至少 50% 的工具成功
    failed > 0 && (total - failed) >= total / 2
}
```

---

### 5.3 何时记录 Hit？

**决策点**: `core/src/gatekeeper/evaluate.rs`

```rust
let shown: HashSet<String> = run.shown_qa_ids.iter().cloned().collect();
let used: HashSet<String> = run.used_qa_ids.iter().cloned().collect();

let mut hit_refs: Vec<HitRef> = Vec::new();

// 记录所有展示过或被使用过的答案
for qa_id in shown.union(&used) {
    hit_refs.push(HitRef {
        qa_id: qa_id.clone(),
        shown: shown.contains(qa_id),
        used: used.contains(qa_id),
        message_id: None,
        context: None,
    });
}

// 若有 hit_refs，调用 record_hit
if !hit_refs.is_empty() {
    let hit_payload = build_hit_payload(project_id, &decision);
    memory.record_hit(hit_payload).await?;
}
```

**使用率计算** (Memory Service 侧):
```python
# Memory Service 伪代码
def update_hit_stats(qa_id, shown, used):
    qa = db.get(qa_id)

    if shown:
        qa.hit_count += 1

    if used:
        qa.use_count += 1

    qa.use_rate = qa.use_count / qa.hit_count if qa.hit_count > 0 else 0.0

    db.save(qa)
```

---

## 6. 典型场景

### 场景 1: 首次执行（无历史答案）

```
Pre-Run:
  Memory.search() → 无匹配 (matches = [])
  Gatekeeper.prepare_inject([]) → inject_list = []
  Merged Prompt = 原始 User Query

Execute:
  Backend 执行
  RunOutcome { exit_code: 0, shown_qa_ids: [], used_qa_ids: [], ... }

Post-Run:
  hit_refs = [] → 无 record_hit
  validate_plans = [] → 无 record_validation
  should_write_candidate = true (无历史答案，质量合格)
  ✅ record_candidate → 创建首个 Candidate (Level 0)
```

---

### 场景 2: 有历史答案，执行成功

```
Pre-Run:
  Memory.search() → matches = [qa-123 (Level 1), qa-456 (Level 0)]
  Gatekeeper.prepare_inject() → inject_list = [qa-123] (仅选高质量)
  shown_qa_ids = ["qa-123"]

Execute:
  Backend 使用 qa-123 的答案
  stdout: "参考 QA-123，修改为..."
  RunOutcome { exit_code: 0, used_qa_ids: ["qa-123"], ... }

Post-Run:
  ✅ record_hit([{ qa_id: "qa-123", shown: true, used: true }])
  ✅ record_validation({ qa_id: "qa-123", result: Pass })
     → qa-123: validation_level 1 → 2, trust 上升
  should_write_candidate = false (has_strong=true, 已有高质量答案)
```

---

### 场景 3: 有历史答案，执行失败

```
Pre-Run:
  inject_list = [qa-123]
  shown_qa_ids = ["qa-123"]

Execute:
  Backend 使用 qa-123 的答案，但执行失败
  RunOutcome { exit_code: 1, used_qa_ids: ["qa-123"], stderr: "Error: ..." }

Post-Run:
  ✅ record_hit([{ qa_id: "qa-123", shown: true, used: true }])
  ✅ record_validation({ qa_id: "qa-123", result: Fail })
     → qa-123: consecutive_fail += 1, trust -= 0.1
     → 若 consecutive_fail >= 3: status → blocked
  should_write_candidate = false (exit_code != 0)
```

---

### 场景 4: 部分成功（混合结果）

```
Execute:
  tool_events = [
    { tool: "write_file", exit_code: 0 },   // 成功
    { tool: "bash", exit_code: 1 },         // 失败
    { tool: "read_file", exit_code: 0 },    // 成功
  ]
  RunOutcome { exit_code: 1, ... }

Post-Run:
  ✅ record_validation({ qa_id: "qa-123", result: Partial })
     → qa-123: consecutive_fail += 0.5, trust += 0.02 (小幅上升)
  should_write_candidate = false (exit_code != 0)
```

---

## 7. 常见问题

### Q1: 为什么 Candidate 和 Validation 是分离的？

**A**: 不同的生命周期和触发条件
- **Candidate**: 记录新答案（来源：本次执行）
- **Validation**: 验证旧答案（来源：历史记录）

一次执行可以：
- 创建 0-N 个 Candidate（本次输出）
- 产生 0-M 个 Validation（验证历史答案）

---

### Q2: Hit 和 Validation 有什么区别？

| 维度 | Hit | Validation |
|------|-----|-----------|
| **记录内容** | 展示/使用统计 | 执行结果评估 |
| **触发条件** | 只要注入或使用 | 仅对 used_qa_ids 验证 |
| **影响指标** | hit_count, use_count | validation_level, trust |
| **目的** | 统计热度 | 评估质量 |

**示例**:
- 答案 A 被展示但未使用 → 记录 Hit (shown=true, used=false)，无 Validation
- 答案 B 被使用且成功 → 记录 Hit (used=true) + Validation (Pass)

---

### Q3: 为什么执行失败时不创建 Candidate？

**A**: 质量控制
- Candidate 用于未来检索，必须保证一定质量
- 失败的输出可能包含错误信息，误导未来查询
- 通过 `should_write_candidate` 的严格条件过滤低质量输出

**例外**: 部分成功场景可能产生有价值的中间结果，但当前实现不支持（未来可扩展）

---

### Q4: consecutive_fail 何时清零？

**A**: 只有 Validation: Pass 会清零

```rust
match validation_result {
    Pass => {
        consecutive_fail = 0;
        trust += 0.05;
    }
    Partial => {
        consecutive_fail += 0.5;
        trust += 0.02;
    }
    Fail => {
        consecutive_fail += 1;
        trust -= 0.10;
    }
}
```

---

### Q5: 如何恢复被 blocked 的答案？

**A**: 两种方式

1. **自然恢复**: 连续 Pass 验证
   ```
   consecutive_fail = 3 (blocked)
   → Validation: Pass
   → consecutive_fail = 0 (恢复)
   ```

2. **手动干预**: 直接修改 Memory Service 数据库
   ```sql
   UPDATE qa_records
   SET consecutive_fail = 0, status = 'active'
   WHERE qa_id = 'qa-123';
   ```

---

### Q6: 如何调整 Candidate 写入的阈值？

**A**: 修改配置文件 `config.toml`

```toml
[candidate_extract]
min_confidence = 0.45          # 最低置信度（默认 0.45）
redact = true                  # 启用敏感信息检测
strict_secret_block = false    # 严格模式：检测到敏感信息直接拒绝

[gatekeeper]
skip_if_top1_score_ge = 0.90   # 已有高分答案时不写入 Candidate
```

---

### Q7: 如何查看某次执行的完整 Memory 写入？

**A**: 检查 `run.events.jsonl` 文件

```bash
# 查看 Memory 相关事件
grep "memory\." run.events.jsonl

# 示例输出
{"v":1,"type":"memory.search.result","ts":"...","data":{"matches":[...]}}
{"v":1,"type":"memory.hit.write","ts":"...","data":{"references":[...]}}
{"v":1,"type":"memory.validation.write","ts":"...","data":{"qa_id":"qa-123","result":"pass"}}
{"v":1,"type":"memory.candidate.write","ts":"...","data":{"confidence":0.85,...}}
```

---

## 总结

Memory 生命周期设计遵循以下原则：

1. **分离关注点**: Candidate（新）、Hit（统计）、Validation（质量）各司其职
2. **质量优先**: 严格的写入条件确保 Candidate 质量
3. **动态调整**: Trust 和 validation_level 根据验证结果实时更新
4. **自我修复**: consecutive_fail 机制自动过滤低质量答案
5. **可追溯**: 所有写入记录在 run.events.jsonl，可审计

**关键文件**:
- `core/src/engine/pre.rs` - Pre-run 搜索和注入
- `core/src/engine/post.rs` - Post-run 写入逻辑
- `core/src/gatekeeper/evaluate.rs` - 决策逻辑
- `core/src/memory/candidates.rs` - Candidate 提取
- `core/src/memory/payloads.rs` - Payload 构建
