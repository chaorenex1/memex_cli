# Memex-CLI 架构分析报告

**生成时间**: 2026-01-10
**范围**: Memory/Gatekeeper 架构设计分析与改进建议

---

## 目录

1. [MemoryPlugin Trait 定义/实现耦合](#1-memoryplugin-trait-定义实现耦合)
2. [Gatekeeper 职责混淆](#2-gatekeeper-职责混淆)
3. [Memory 概念双实现路径](#3-memory-概念双实现路径)
4. [Candidate/Hit/Validation 生命周期](#4-candidatehitvalidation-生命周期)

---

## 1. MemoryPlugin Trait 定义/实现耦合

### 1.1 问题描述

**当前架构**:

```
core/src/memory/
├── trait.rs          # MemoryPlugin trait 定义
├── client.rs         # MemoryClient HTTP 实现（⚠️ 问题所在）
├── models.rs         # Payload 数据结构
└── ...

plugins/src/memory/
└── service.rs        # MemoryServicePlugin（薄封装层）
```

**核心问题**:

1. **MemoryClient** (HTTP 实现) 位于 `core/src/memory/client.rs`
   - Core 应该是抽象层，不应包含具体的 HTTP 实现
   - 导致 core 依赖 `reqwest`（HTTP 客户端库）
   - 违反依赖倒置原则 (DIP)

2. **MemoryServicePlugin** 只是一个薄封装
   - `plugins/src/memory/service.rs` 仅封装 MemoryClient
   - 没有实质性逻辑，职责不清晰

**代码示例**:

```rust
// core/src/memory/client.rs (❌ 不应在 core)
pub struct MemoryClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,  // ⚠️ core 依赖 HTTP 实现细节
}

// plugins/src/memory/service.rs (薄封装)
pub struct MemoryServicePlugin {
    client: core_api::MemoryClient,  // 直接使用 core 的 HTTP client
}
```

### 1.2 影响分析

| 影响维度 | 严重程度 | 说明 |
|---------|---------|------|
| **依赖反转** | ⭐⭐⭐⭐ | Core 依赖具体实现（reqwest），违反分层原则 |
| **可测试性** | ⭐⭐⭐ | HTTP client 耦合导致测试需要 mock HTTP |
| **可扩展性** | ⭐⭐ | 新增 Memory 实现（如 gRPC）需修改 core |
| **代码复用** | ⭐ | MemoryServicePlugin 代码冗余 |

### 1.3 改进方案

#### 方案 A: 移动 MemoryClient 到 Plugins（推荐）

**目标架构**:

```
core/src/memory/
├── trait.rs          # MemoryPlugin trait (抽象接口)
├── models.rs         # Payload 数据结构
└── ...               # 无 HTTP 实现

plugins/src/memory/
├── service.rs        # MemoryServicePlugin 完整实现
└── http_client.rs    # HTTP client（从 core 移出）
```

**优势**:
- ✅ Core 完全解耦 HTTP 实现
- ✅ 符合 DIP（依赖倒置原则）
- ✅ 新增实现无需修改 core

**实施步骤**:

1. 将 `core/src/memory/client.rs` → `plugins/src/memory/http_client.rs`
2. 删除 core 对 `reqwest` 的依赖
3. 整合 `MemoryServicePlugin` 和 `MemoryClient`
4. 更新 `core/src/api.rs` 移除 MemoryClient 导出

**预估工作量**: 约 80-100 行重构，影响 4-5 个文件

#### 方案 B: 抽象 HTTP 层（备选）

引入 `HttpClient` trait 作为中间抽象：

```rust
// core/src/http.rs
pub trait HttpClient: Send + Sync {
    async fn post_json(&self, url: &str, body: Value) -> Result<Value>;
}

// plugins/src/http/reqwest.rs
pub struct ReqwestClient { ... }
impl HttpClient for ReqwestClient { ... }
```

**缺点**: 增加复杂度，收益不大（Memory 是唯一的 HTTP 服务）

### 1.4 建议

**推荐方案 A**，理由：
- Memory Service 是外部 HTTP 服务，完整实现应在 plugins
- Core 只需定义接口（MemoryPlugin trait）
- 更清晰的职责分离

---

## 2. Gatekeeper 职责混淆

### 2.1 问题描述

**当前 GatekeeperPlugin trait**:

```rust
pub trait GatekeeperPlugin {
    fn prepare_inject(&self, matches: &[SearchMatch]) -> Vec<InjectItem>;  // ✅ 已拆分

    fn evaluate(
        &self,
        now: DateTime<Local>,
        matches: &[SearchMatch],
        outcome: &RunOutcome,
        events: &[ToolEvent],
    ) -> GatekeeperDecision;  // ⚠️ 职责过多
}
```

**GatekeeperDecision 包含的决策**:

```rust
pub struct GatekeeperDecision {
    pub inject_list: Vec<InjectItem>,        // 1. 注入筛选（已拆分到 prepare_inject）
    pub hit_refs: Vec<HitRef>,               // 2. 命中记录决策
    pub validate_plans: Vec<ValidatePlan>,   // 3. 验证计划决策
    pub should_write_candidate: bool,        // 4. 候选写入决策
    pub reasons: Vec<String>,                // 5. 决策理由
    pub tool_insights: serde_json::Value,    // 6. 工具洞察
}
```

**职责分析**:

| 职责 | 当前归属 | 应归属 | 说明 |
|------|---------|--------|------|
| **注入筛选** | `prepare_inject` ✅ | Gatekeeper | 已正确拆分 |
| **质量评估** | `evaluate` | Gatekeeper | 核心职责 |
| **命中记录** | `evaluate.hit_refs` | Memory Writer | 应独立决策 |
| **验证计划** | `evaluate.validate_plans` | Memory Writer | 应独立决策 |
| **候选写入** | `evaluate.should_write_candidate` | Memory Writer | 应独立决策 |

**核心问题**: `evaluate` 同时承担 **Quality Gate** 和 **Memory Write Decision** 两种职责

### 2.2 影响分析

| 影响维度 | 严重程度 | 说明 |
|---------|---------|------|
| **单一职责原则** | ⭐⭐⭐⭐ | evaluate 职责过多，难以理解和维护 |
| **可测试性** | ⭐⭐⭐ | 测试需覆盖多个决策维度 |
| **可扩展性** | ⭐⭐⭐ | 新增 Memory 决策逻辑需修改 Gatekeeper |
| **代码复用** | ⭐⭐ | Memory write 逻辑与 Gatekeeper 耦合 |

### 2.3 当前代码示例

**Gatekeeper evaluate 返回所有决策**:

```rust
// core/src/gatekeeper/evaluate.rs
impl Gatekeeper {
    pub fn evaluate(...) -> GatekeeperDecision {
        // 1. 质量评估（核心职责）
        let quality_signals = build_signals(...);

        // 2. 命中记录决策（应独立）
        let hit_refs: Vec<HitRef> = usable.iter()
            .filter(|m| run_outcome.shown_qa_ids.contains(&m.qa_id))
            .map(|m| HitRef { qa_id: m.qa_id.clone(), used: ... })
            .collect();

        // 3. 验证计划决策（应独立）
        let validate_plans: Vec<ValidatePlan> = ...;

        // 4. 候选写入决策（应独立）
        let should_write_candidate = run.exit_code == 0 && quality_signals.pass;

        GatekeeperDecision {
            inject_list,      // 已拆分到 prepare_inject
            hit_refs,         // ⚠️ 应独立
            validate_plans,   // ⚠️ 应独立
            should_write_candidate,  // ⚠️ 应独立
            ...
        }
    }
}
```

**post.rs 使用全部决策**:

```rust
// core/src/engine/post.rs
let decision = gatekeeper.evaluate(...);

// 直接使用所有决策
if let Some(hit) = build_hit_payload(&decision) {
    mem.record_hit(hit).await?;
}
for v in build_validate_payloads(&decision) {
    mem.record_validation(v).await?;
}
if decision.should_write_candidate {
    mem.record_candidate(...).await?;
}
```

### 2.4 改进方案

#### 方案 A: 拆分为两个独立决策层（推荐）

**新架构**:

```rust
// 1. Gatekeeper: 纯质量评估
pub trait GatekeeperPlugin {
    fn prepare_inject(&self, matches: &[SearchMatch]) -> Vec<InjectItem>;

    fn assess_quality(  // 重命名 evaluate → assess_quality
        &self,
        matches: &[SearchMatch],
        outcome: &RunOutcome,
        events: &[ToolEvent],
    ) -> QualityAssessment;  // 新结构：只包含质量信号
}

pub struct QualityAssessment {
    pub signals: QualitySignals,
    pub insights: ToolInsights,
    pub reasons: Vec<String>,
}

// 2. MemoryWriter: 独立的写入决策器
pub struct MemoryWriteDecider;

impl MemoryWriteDecider {
    pub fn decide(
        assessment: &QualityAssessment,
        matches: &[SearchMatch],
        outcome: &RunOutcome,
    ) -> MemoryWritePlan {
        MemoryWritePlan {
            hit_refs: self.decide_hits(matches, outcome),
            validate_plans: self.decide_validations(assessment, matches),
            should_write_candidate: self.decide_candidate(assessment, outcome),
        }
    }
}
```

**调用流程**:

```rust
// core/src/engine/post.rs
let assessment = gatekeeper.assess_quality(matches, &run_outcome, &events);
let write_plan = MemoryWriteDecider::decide(&assessment, matches, &run_outcome);

if let Some(hit) = build_hit_payload(&write_plan.hit_refs) {
    mem.record_hit(hit).await?;
}
// ...
```

**优势**:
- ✅ 职责清晰：Gatekeeper = 质量评估，Decider = 写入决策
- ✅ 可独立测试
- ✅ 可独立替换（如切换不同的写入策略）

**预估工作量**: 约 150-200 行重构，影响 6-8 个文件

#### 方案 B: 保持现状 + 文档说明（不推荐）

仅添加注释说明 `evaluate` 的多重职责，不重构代码。

**缺点**: 技术债持续累积

### 2.5 建议

**推荐方案 A**，分阶段实施：

**阶段 1**: 提取 MemoryWriteDecider（内部重构，不改 trait）
- 在 `post.rs` 内部封装决策逻辑
- 保持对外接口不变

**阶段 2**: 重构 GatekeeperPlugin trait（破坏性变更）
- 拆分 `assess_quality` 和 `prepare_inject`
- 需要主版本升级

---

## 3. Memory 概念双实现路径

### 3.1 问题描述

**当前分层**:

```
core/src/memory/           # 抽象层
├── trait.rs               # MemoryPlugin trait
├── models.rs              # QASearchPayload, QACandidatePayload, ...
├── client.rs              # MemoryClient (HTTP 实现)
├── payloads.rs            # Payload 构建函数
├── extract.rs             # Candidate 提取逻辑
└── parse.rs               # SearchMatch 解析

plugins/src/memory/        # 实现层
└── service.rs             # MemoryServicePlugin (薄封装)
```

**理解成本高的原因**:

1. **分层目的不清晰**:
   - Core 既有抽象（trait）又有实现（client）
   - Plugins 只是薄封装，看不出存在价值

2. **职责分散**:
   - Payload 构建在 core（`payloads.rs`）
   - Payload 定义在 core（`models.rs`）
   - Payload 发送在 plugins（`service.rs` → `client.rs`）

3. **命名混淆**:
   - `MemoryClient` vs `MemoryPlugin` vs `MemoryServicePlugin`
   - 三者关系不明确

### 3.2 当前设计意图（推测）

| 组件 | 设计意图 | 实际情况 |
|------|---------|---------|
| **core/memory/** | 定义 Memory 抽象接口 | ✅ 包含 trait 和 models |
| | 提供工具函数（parse/extract） | ✅ 提供 parse/extract 工具 |
| | **不应包含实现细节** | ❌ 包含 HTTP client 实现 |
| **plugins/memory/** | 实现 MemoryPlugin trait | ❌ 只是薄封装，无实质逻辑 |
| | 处理 HTTP 通信细节 | ❌ HTTP 逻辑在 core/client.rs |

### 3.3 改进方案

#### 方案 A: 明确分层职责（推荐）

**目标架构**:

```
core/src/memory/           # 纯抽象层
├── trait.rs               # MemoryPlugin trait (抽象接口)
├── models.rs              # Payload 数据结构（DTO）
├── payloads.rs            # Payload 构建工具（业务逻辑）
├── extract.rs             # Candidate 提取（业务逻辑）
└── parse.rs               # SearchMatch 解析（业务逻辑）

plugins/src/memory/        # 完整实现层
├── service.rs             # MemoryServicePlugin (整合实现)
└── http_client.rs         # HTTP client (从 core 移出)
```

**职责划分**:

| 层次 | 职责 | 示例 |
|------|-----|------|
| **core/memory** | 定义接口 | `trait MemoryPlugin` |
| | 定义数据模型 | `QASearchPayload`, `SearchMatch` |
| | 提供业务逻辑工具 | `extract_candidates`, `parse_search_matches` |
| **plugins/memory** | 实现接口 | `impl MemoryPlugin for MemoryServicePlugin` |
| | 处理通信细节 | HTTP client, 序列化/反序列化 |

**实施步骤**:

1. 移动 `core/src/memory/client.rs` → `plugins/src/memory/http_client.rs`
2. 整合 `MemoryServicePlugin` 和 `MemoryClient`（去除薄封装）
3. 添加架构文档 `docs/MEMORY_ARCHITECTURE.md`

**预估工作量**: 约 100 行重构 + 文档

#### 方案 B: 添加架构文档（立即可行）

不修改代码，仅添加文档说明当前分层设计意图。

**文档内容**:
- 为何 MemoryClient 在 core（历史原因？）
- 各组件职责边界
- 新增实现指南（如何添加 gRPC Memory）

### 3.4 建议

**立即执行**: 方案 B（添加文档）
**中期优化**: 方案 A（重构分层）+ 配合问题 #1 的方案 A

---

## 4. Candidate/Hit/Validation 生命周期

### 4.1 概述

Memory 系统中的三种记录类型各有独立的生命周期和转换规则。

### 4.2 数据模型

```rust
// 1. Candidate（候选答案）
pub struct QACandidatePayload {
    pub project_id: String,
    pub query: String,          // 用户问题
    pub answer: String,         // AI 回答
    pub context: String,        // 上下文（stdout/stderr/tool_events）
    pub tags: Vec<String>,      // 标签
    pub confidence: f32,        // 置信度 (0.0-1.0)
}

// 2. Validation（验证结果）
pub struct QAValidationPayload {
    pub project_id: String,
    pub qa_id: String,          // 关联的 QA 记录 ID
    pub result: ValidationResult,  // pass | fail | partial
    pub notes: Option<String>,  // 验证备注
}

// 3. Hit（命中记录）
pub struct QAHitsPayload {
    pub project_id: String,
    pub references: Vec<QAReferencePayload>,  // 多个 QA 引用
}

pub struct QAReferencePayload {
    pub qa_id: String,
    pub shown: Option<bool>,    // 是否展示给用户
    pub used: Option<bool>,     // 是否被 AI 使用
}
```

### 4.3 完整生命周期

```
┌──────────────────────────────────────────────────────────────────┐
│                     Memory Lifecycle                              │
└──────────────────────────────────────────────────────────────────┘

【阶段 1: Pre-Run】搜索记忆
────────────────────────────────────────────────────────────────────
User Query
    ↓
Memory.search(QASearchPayload)
    ↓
SearchMatch[] ← Memory Service 返回历史 QA 记录
    ↓
Gatekeeper.prepare_inject(matches)
    ↓
InjectItem[] ← 筛选高质量记录注入到 prompt
    ↓
Merged Prompt = User Query + InjectItem[]


【阶段 2: Execute】执行
────────────────────────────────────────────────────────────────────
Backend (Claude/Codex/Gemini) 执行
    ↓
ToolEvent[] + stdout + stderr
    ↓
RunOutcome {
    exit_code,
    tool_events,
    stdout_tail,
    stderr_tail,
    shown_qa_ids,   ← 从 prompt 提取 QA 引用
    used_qa_ids,    ← 从 stdout 提取 QA 引用
}


【阶段 3: Post-Run】质量评估与记忆写入
────────────────────────────────────────────────────────────────────
Gatekeeper.evaluate(matches, outcome, tool_events)
    ↓
GatekeeperDecision {
    hit_refs: Vec<HitRef>,               ← 3.1 命中决策
    validate_plans: Vec<ValidatePlan>,   ← 3.2 验证决策
    should_write_candidate: bool,        ← 3.3 候选决策
}
    ↓
    ├─→ 3.1 Record Hit（命中记录）
    │   ────────────────────────────────────────────
    │   if hit_refs.len() > 0 {
    │       Memory.record_hit(QAHitsPayload {
    │           references: [
    │               { qa_id: "xxx", shown: true, used: false },
    │               { qa_id: "yyy", shown: true, used: true },
    │           ]
    │       })
    │   }
    │
    │   用途:
    │   - 追踪哪些历史答案被展示给用户
    │   - 追踪哪些历史答案被 AI 实际使用
    │   - 计算答案的使用率 (hit rate)
    │
    ├─→ 3.2 Record Validation（验证结果）
    │   ────────────────────────────────────────────
    │   for plan in validate_plans {
    │       Memory.record_validation(QAValidationPayload {
    │           qa_id: plan.qa_id,
    │           result: plan.result,  // pass | fail | partial
    │           notes: plan.notes,
    │       })
    │   }
    │
    │   用途:
    │   - 记录历史答案在本次执行中的表现
    │   - 更新答案的 validation_level (0-3)
    │   - 影响未来搜索的排序和筛选
    │
    │   触发条件:
    │   - 执行成功 (exit_code == 0) → validation_result = pass
    │   - 执行失败 (exit_code != 0) → validation_result = fail
    │   - 部分成功 → validation_result = partial
    │
    └─→ 3.3 Record Candidate（候选答案）
        ────────────────────────────────────────────
        if should_write_candidate {
            candidates = extract_candidates(
                user_query,
                stdout_tail,
                stderr_tail,
                tool_events
            )

            for c in candidates {
                Memory.record_candidate(QACandidatePayload {
                    query: user_query,
                    answer: c.answer,     ← 从输出中提取
                    context: c.context,   ← stdout + stderr + tool_events
                    confidence: c.confidence,
                    tags: c.tags,
                })
            }
        }

        用途:
        - 将本次执行的输出作为新的答案候选
        - 供未来查询检索使用
        - 候选可通过 Validation 升级为 Verified

        触发条件（所有条件需同时满足）:
        - exit_code == 0 (执行成功)
        - tool_events 包含实质性内容 (非空操作)
        - 输出符合质量标准 (confidence >= threshold)
        - 无安全风险（未检测到敏感信息）


【阶段 4: Storage】持久化
────────────────────────────────────────────────────────────────────
Memory Service (外部 HTTP 服务)
    ↓
Database (PostgreSQL/SQLite/...)
    ↓
QA Records with:
    - validation_level: 0 (candidate) → 1 (verified) → 2 (confirmed) → 3 (gold)
    - consecutive_fail: 连续失败次数（影响 trust）
    - trust: 信任度 (0.0-1.0)
    - freshness: 新鲜度（时间衰减）
    - hit_count: 命中次数
    - use_count: 实际使用次数
```

### 4.4 状态转换图

```
Candidate (Level 0)
    ↓
    │ Validation: pass
    ↓
Verified (Level 1)
    ↓
    │ Validation: pass (multiple times)
    ↓
Confirmed (Level 2)
    ↓
    │ Validation: pass (高频使用)
    ↓
Gold Standard (Level 3)


任意 Level:
    │ Validation: fail
    ↓
consecutive_fail += 1
trust -= penalty

if consecutive_fail >= block_threshold:
    Status: blocked (不再注入)
```

### 4.5 关键决策逻辑

#### 4.5.1 何时写入 Candidate？

**决策点**: `core/src/gatekeeper/evaluate.rs`

```rust
let should_write_candidate =
    run.exit_code == 0                    // 执行成功
    && !tool_events.is_empty()            // 有实质性操作
    && quality_signals.confidence >= 0.45 // 置信度阈值
    && !has_secrets                       // 无敏感信息
    && !is_trivial_output                 // 非琐碎输出
```

**提取逻辑**: `core/src/memory/extract.rs`

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

    if confidence < config.confidence {
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

#### 4.5.2 何时记录 Validation？

**决策点**: `core/src/gatekeeper/evaluate.rs`

```rust
let validate_plans: Vec<ValidatePlan> = matches
    .iter()
    .filter(|m| shown_qa_ids.contains(&m.qa_id))  // 只验证展示的答案
    .map(|m| {
        let result = if run.exit_code == 0 {
            ValidationResult::Pass  // 成功 → pass
        } else if partial_success_detected(&run.tool_events) {
            ValidationResult::Partial  // 部分成功 → partial
        } else {
            ValidationResult::Fail  // 失败 → fail
        };

        ValidatePlan {
            qa_id: m.qa_id.clone(),
            result,
            notes: Some(build_validation_notes(&run)),
        }
    })
    .collect();
```

#### 4.5.3 何时记录 Hit？

**决策点**: `core/src/gatekeeper/evaluate.rs`

```rust
let hit_refs: Vec<HitRef> = matches
    .iter()
    .map(|m| {
        let shown = shown_qa_ids.contains(&m.qa_id);
        let used = used_qa_ids.contains(&m.qa_id);  // 从 stdout 提取

        HitRef {
            qa_id: m.qa_id.clone(),
            shown: Some(shown),
            used: Some(used),
        }
    })
    .collect();
```

**Used 检测**: `core/src/gatekeeper/extract.rs`

```rust
pub fn extract_qa_refs(stdout: &str) -> Vec<String> {
    // 匹配模式: [QA:xxx] 或 (ref:xxx)
    let re = Regex::new(r"\[QA:([a-zA-Z0-9_-]+)\]|\(ref:([a-zA-Z0-9_-]+)\)").unwrap();

    re.captures_iter(stdout)
        .filter_map(|cap| {
            cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str().to_string())
        })
        .collect()
}
```

### 4.6 典型场景示例

#### 场景 1: 首次提问（无历史答案）

```
Pre-Run:
  Memory.search("如何配置 Rust logger?")
  → matches = [] (无历史)
  → inject_list = []
  → Merged Prompt = "如何配置 Rust logger?"

Execute:
  Backend 执行生成答案
  → exit_code = 0
  → tool_events = [...]
  → stdout = "使用 tracing crate:\n[code]..."

Post-Run:
  Gatekeeper.evaluate()
  → hit_refs = [] (无历史答案)
  → validate_plans = [] (无历史答案)
  → should_write_candidate = true (执行成功)

  extract_candidates()
  → candidates = [{
      query: "如何配置 Rust logger?",
      answer: "使用 tracing crate:...",
      confidence: 0.87
  }]

  Memory.record_candidate(candidates[0])
  → 新 QA 记录创建，validation_level = 0 (candidate)
```

#### 场景 2: 相似问题（有历史答案）

```
Pre-Run:
  Memory.search("Rust logger 配置教程")
  → matches = [
      { qa_id: "qa_001", validation_level: 0, trust: 0.5, answer: "..." },
  ]
  → inject_list = [qa_001] (筛选注入)
  → Merged Prompt = "Rust logger 配置教程\n\n参考答案:\n[QA:qa_001] ..."

Execute:
  Backend 执行（参考了历史答案）
  → exit_code = 0
  → stdout = "参考 [QA:qa_001] 的方法，添加 tracing-subscriber..."

Post-Run:
  Gatekeeper.evaluate()
  → hit_refs = [
      { qa_id: "qa_001", shown: true, used: true }  ← 检测到 [QA:qa_001]
  ]
  → validate_plans = [
      { qa_id: "qa_001", result: pass }  ← 执行成功，验证通过
  ]
  → should_write_candidate = false (参考了历史答案，未产生新答案)

  Memory.record_hit(hit_refs)
  → qa_001.hit_count += 1, use_count += 1

  Memory.record_validation(validate_plans[0])
  → qa_001.validation_level: 0 → 1 (candidate → verified)
  → qa_001.consecutive_fail = 0 (重置失败计数)
```

#### 场景 3: 答案失效（历史答案过时）

```
Pre-Run:
  Memory.search("如何使用 Rust async?")
  → matches = [
      { qa_id: "qa_005", validation_level: 2, trust: 0.85, answer: "使用 tokio 0.2..." },
  ]
  → inject_list = [qa_005]

Execute:
  Backend 执行（参考过时答案导致失败）
  → exit_code = 1 (编译错误)
  → stderr = "tokio 0.2 API 已过时，使用 tokio 1.0"

Post-Run:
  Gatekeeper.evaluate()
  → hit_refs = [
      { qa_id: "qa_005", shown: true, used: true }
  ]
  → validate_plans = [
      { qa_id: "qa_005", result: fail, notes: "API 过时" }
  ]
  → should_write_candidate = false (执行失败)

  Memory.record_validation(validate_plans[0])
  → qa_005.consecutive_fail += 1 (1)
  → qa_005.trust -= 0.1 (0.85 → 0.75)
  → 如果 consecutive_fail >= 3: qa_005.status = blocked
```

### 4.7 配置参数

**Gatekeeper 配置** (`config.toml`):

```toml
[gatekeeper]
max_inject = 3                    # 最多注入 3 条历史答案
min_trust_show = 0.40             # 信任度 >= 0.4 才注入
min_level_inject = 2              # validation_level >= 2 优先注入
min_level_fallback = 1            # 无高质量答案时降级到 level >= 1
block_if_consecutive_fail_ge = 3  # 连续失败 >= 3 次则屏蔽
skip_if_top1_score_ge = 0.85      # 最高分 >= 0.85 则跳过低分答案
```

**Candidate Extract 配置**:

```toml
[candidate_extract]
max_candidates = 1                # 每次最多提取 1 个候选
max_answer_chars = 1200           # 答案最大长度
min_answer_chars = 200            # 答案最小长度
confidence = 0.45                 # 置信度阈值
redact = true                     # 启用敏感信息脱敏
strict_secret_block = true        # 检测到敏感信息直接拒绝
```

---

## 5. 总结与优先级建议

### 5.1 问题优先级

| 问题 | 优先级 | 预估工作量 | 建议时间 |
|------|--------|-----------|----------|
| **#4 生命周期文档** | ⭐⭐⭐⭐⭐ | 已完成 | ✅ 立即可用 |
| **#3 Memory 分层文档** | ⭐⭐⭐⭐ | 1 小时 | 本周内 |
| **#1 MemoryClient 解耦** | ⭐⭐⭐⭐ | 3-4 小时 | 1-2 周内 |
| **#2 Gatekeeper 职责拆分** | ⭐⭐⭐ | 5-6 小时 | 1 月内 |

### 5.2 立即行动项

1. ✅ **本文档已完成** - 提供架构分析和生命周期文档
2. **添加 Memory 分层文档** - 说明当前设计意图
3. **规划 MemoryClient 重构** - 配合依赖解耦

### 5.3 长期改进路线

**Phase 1 (文档完善)**:
- 添加 `docs/MEMORY_ARCHITECTURE.md` 说明分层设计
- 更新 `CLAUDE.md` 添加架构章节

**Phase 2 (解耦重构)**:
- 移动 MemoryClient 到 plugins
- 移除 core 对 reqwest 的依赖

**Phase 3 (职责分离)**:
- 提取 MemoryWriteDecider
- 拆分 Gatekeeper.evaluate

---

**文档版本**: v1.0
**最后更新**: 2026-01-10
**作者**: Architecture Analysis Task
