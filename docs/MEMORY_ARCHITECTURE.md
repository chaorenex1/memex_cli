# Memory 架构设计文档

**目的**: 说明 Memex-CLI Memory 系统的分层设计、职责划分和扩展指南

---

## 目录

1. [架构概览](#1-架构概览)
2. [分层职责](#2-分层职责)
3. [数据流](#3-数据流)
4. [扩展指南](#4-扩展指南)

---

## 1. 架构概览

### 1.1 设计原则

Memory 系统遵循以下设计原则：

1. **依赖倒置** (DIP): Core 定义抽象接口，Plugins 提供具体实现
2. **单一职责** (SRP): 每个模块专注于一个明确的职责
3. **开放封闭** (OCP): 可扩展新实现（如 gRPC），无需修改 core

### 1.2 当前架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                         Application Layer                        │
│  (cli/src/flow/, cli/src/app.rs)                                │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                          Engine Layer                            │
│  (core/src/engine/)                                              │
│  ┌─────────────┐   ┌──────────────┐   ┌──────────────┐         │
│  │  pre.rs     │ → │   run.rs     │ → │   post.rs    │         │
│  │  (search)   │   │  (execute)   │   │  (write)     │         │
│  └─────────────┘   └──────────────┘   └──────────────┘         │
│         ↓                                      ↓                 │
│    MemoryPlugin::search              MemoryPlugin::record_*     │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                    Abstraction Layer (Core)                      │
│  (core/src/memory/)                                              │
│  ┌──────────────────────────────────────────────────────┐       │
│  │  trait.rs                                             │       │
│  │  ┌─────────────────────────────────────────────┐    │       │
│  │  │  pub trait MemoryPlugin {                   │    │       │
│  │  │      async fn search(...) -> ...;           │    │       │
│  │  │      async fn record_hit(...) -> ...;       │    │       │
│  │  │      async fn record_candidate(...) -> ...; │    │       │
│  │  │      async fn record_validation(...) -> ... │    │       │
│  │  │  }                                           │    │       │
│  │  └─────────────────────────────────────────────┘    │       │
│  └──────────────────────────────────────────────────────┘       │
│  ┌──────────────────────────────────────────────────────┐       │
│  │  models.rs (Data Transfer Objects)                   │       │
│  │  - QASearchPayload, QACandidatePayload, ...          │       │
│  │  - SearchMatch (search result)                       │       │
│  └──────────────────────────────────────────────────────┘       │
│  ┌──────────────────────────────────────────────────────┐       │
│  │  Utilities (Business Logic Helpers)                  │       │
│  │  - payloads.rs: build_*_payload()                    │       │
│  │  - candidates.rs: extract_candidates()               │       │
│  │  - adapters.rs: parse_search_matches()               │       │
│  └──────────────────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                  Implementation Layer (Plugins)                  │
│  (plugins/src/memory/)                                           │
│  ┌──────────────────────────────────────────────────────┐       │
│  │  http_client.rs (HTTP Communication)                 │       │
│  │  ┌──────────────────────────────────────────────┐   │       │
│  │  │  pub struct HttpClient {                     │   │       │
│  │  │      base_url: String,                       │   │       │
│  │  │      api_key: String,                        │   │       │
│  │  │      http: reqwest::Client,                  │   │       │
│  │  │  }                                            │   │       │
│  │  │                                               │   │       │
│  │  │  impl HttpClient {                           │   │       │
│  │  │      pub async fn search(...) -> ...;        │   │       │
│  │  │      pub async fn send_hit(...) -> ...;      │   │       │
│  │  │      pub async fn send_candidate(...) -> ...; │   │       │
│  │  │      pub async fn send_validate(...) -> ...;  │   │       │
│  │  │  }                                            │   │       │
│  │  └──────────────────────────────────────────────┘   │       │
│  └──────────────────────────────────────────────────────┘       │
│  ┌──────────────────────────────────────────────────────┐       │
│  │  service.rs (MemoryPlugin Implementation)            │       │
│  │  ┌──────────────────────────────────────────────┐   │       │
│  │  │  pub struct MemoryServicePlugin {            │   │       │
│  │  │      client: HttpClient,  // ✅ from plugins │   │       │
│  │  │  }                                            │   │       │
│  │  │                                               │   │       │
│  │  │  impl MemoryPlugin for MemoryServicePlugin { │   │       │
│  │  │      async fn search(...) {                  │   │       │
│  │  │          self.client.search(...).await       │   │       │
│  │  │      }                                        │   │       │
│  │  │      // ... other methods                    │   │       │
│  │  │  }                                            │   │       │
│  │  └──────────────────────────────────────────────┘   │       │
│  └──────────────────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                      External Service                            │
│  Memory Service (HTTP API)                                       │
│  - POST /v1/qa/search                                            │
│  - POST /v1/qa/hit                                               │
│  - POST /v1/qa/candidates                                        │
│  - POST /v1/qa/validate                                          │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 分层职责

### 2.1 Core Layer (`core/src/memory/`)

**职责**: 定义 Memory 抽象和业务逻辑工具

#### 2.1.1 `trait.rs` - 抽象接口

```rust
#[async_trait]
pub trait MemoryPlugin: Send + Sync {
    fn name(&self) -> &str;

    // 搜索历史 QA
    async fn search(&self, payload: QASearchPayload) -> Result<Vec<SearchMatch>>;

    // 记录命中
    async fn record_hit(&self, payload: QAHitsPayload) -> Result<()>;

    // 记录候选答案
    async fn record_candidate(&self, payload: QACandidatePayload) -> Result<()>;

    // 记录验证结果
    async fn record_validation(&self, payload: QAValidationPayload) -> Result<()>;

    // 任务分级（可选功能）
    async fn task_grade(&self, prompt: String) -> Result<TaskGradeResult>;
}
```

**设计意图**:
- 定义 Memory 操作的最小接口
- 不关心实现细节（HTTP/gRPC/本地存储）
- 所有实现必须遵循此接口契约

#### 2.1.2 `models.rs` - 数据传输对象 (DTO)

**定义的结构体**:

```rust
// 请求 Payload
pub struct QASearchPayload { ... }      // 搜索请求
pub struct QAHitsPayload { ... }        // 命中记录请求
pub struct QACandidatePayload { ... }   // 候选答案请求
pub struct QAValidationPayload { ... }  // 验证结果请求

// 响应 Model
pub struct SearchMatch { ... }          // 搜索结果
pub struct TaskGradeResult { ... }      // 任务分级结果
```

**设计意图**:
- 定义跨层传递的数据结构
- 保持序列化兼容性（serde）
- 作为 Core 与 Plugins 之间的契约

#### 2.1.3 `payloads.rs` - Payload 构建工具

**提供函数**:

```rust
pub fn build_hit_payload(project_id: &str, decision: &GatekeeperDecision)
    -> Option<QAHitsPayload>;

pub fn build_validate_payloads(project_id: &str, decision: &GatekeeperDecision)
    -> Vec<QAValidationPayload>;

pub fn build_candidate_payloads(project_id: &str, drafts: &[CandidateDraft])
    -> Vec<QACandidatePayload>;
```

**设计意图**:
- 封装从业务数据到 Payload 的转换逻辑
- 复用跨多个调用点的构建代码
- 包含业务规则（如 hit_refs 去重）

#### 2.1.4 `extract.rs` - Candidate 提取逻辑

**核心函数**:

```rust
pub fn extract_candidates(
    config: &CandidateExtractConfig,
    query: &str,
    stdout: &str,
    stderr: &str,
    tool_events: &[ToolEventLite],
) -> Vec<CandidateDraft>;
```

**设计意图**:
- 从执行输出中提取可复用的答案
- 应用质量检查（置信度、敏感信息检测）
- 属于业务逻辑，不依赖具体存储实现

#### 2.1.5 `adapters.rs` - SearchMatch 解析

**核心函数**:

```rust
pub fn parse_search_matches(json: &serde_json::Value) -> Result<Vec<SearchMatch>, String>;
```

**设计意图**:
- 将 Memory Service 返回的 JSON 转换为 SearchMatch
- 处理向后兼容性（字段缺失、类型转换）
- 隔离外部 API 格式变化

---

### 2.2 Plugins Layer (`plugins/src/memory/`)

**职责**: 实现 MemoryPlugin trait，处理通信细节

#### 2.2.1 `http_client.rs` - HTTP Communication

**实现**:

```rust
pub struct HttpClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,  // ✅ HTTP 实现细节在 plugins
}

impl HttpClient {
    pub async fn search(&self, payload: QASearchPayload) -> Result<Value> {
        let url = format!("{}/v1/qa/search", self.base_url.trim_end_matches('/'));
        let req = self.http.post(url).json(&payload);
        let resp = self.auth(req).send().await?;
        resp.json().await
    }

    pub async fn send_hit(&self, payload: QAHitsPayload) -> Result<Value> { ... }
    pub async fn send_candidate(&self, payload: QACandidatePayload) -> Result<Value> { ... }
    pub async fn send_validate(&self, payload: QAValidationPayload) -> Result<Value> { ... }
    pub async fn task_grade(&self, prompt: String) -> Result<Value> { ... }
}
```

**设计意图**:
- 封装所有 HTTP 通信细节（URL 构建、认证、序列化）
- 与 Memory Service API 的完整对接
- 独立于 core，方便替换实现（gRPC、本地存储等）

#### 2.2.2 `service.rs` - MemoryServicePlugin

**实现**:

```rust
pub struct MemoryServicePlugin {
    client: HttpClient,  // ✅ 使用 plugins 层的 HTTP client
}

impl MemoryPlugin for MemoryServicePlugin {
    async fn search(&self, payload: QASearchPayload) -> Result<Vec<SearchMatch>> {
        let raw = self.client.search(payload).await?;
        core_api::parse_search_matches(&raw)
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn record_hit(&self, payload: QAHitsPayload) -> Result<()> {
        self.client.send_hit(payload).await?;
        Ok(())
    }

    async fn record_candidate(&self, payload: QACandidatePayload) -> Result<()> {
        self.client.send_candidate(payload).await?;
        Ok(())
    }

    async fn record_validation(&self, payload: QAValidationPayload) -> Result<()> {
        self.client.send_validate(payload).await?;
        Ok(())
    }

    async fn task_grade(&self, prompt: String) -> Result<TaskGradeResult> {
        let raw = self.client.task_grade(prompt).await?;
        serde_json::from_value(raw)
            .map_err(|e| anyhow::anyhow!("Failed to parse TaskGradeResult: {}", e))
    }
}
```

**设计意图**:
- 实现 MemoryPlugin trait，桥接 core 抽象和 HTTP 通信
- 负责错误转换（HTTP 错误 → anyhow::Error）
- 调用 core 工具函数（parse_search_matches）进行数据转换

**职责边界**:
- ✅ **Plugins 层负责**：HTTP 通信、序列化/反序列化、错误处理
- ✅ **Core 层负责**：业务逻辑（parse、extract、payload 构建）
- 可单独测试 HTTP 通信

---

## 3. 数据流

### 3.1 Search 流程

```
┌─────────────────────────────────────────────────────────────────┐
│  1. Pre-Run Phase (core/src/engine/pre.rs)                      │
└─────────────────────────────────────────────────────────────────┘
User Query: "如何配置 Rust logger?"
    ↓
EngineContext {
    memory: Some(&dyn MemoryPlugin),  ← 注入的 MemoryServicePlugin
    project_id: "my-project",
    ...
}
    ↓
pre_run(&ctx, &user_query)
    ↓
    ├─→ Build QASearchPayload
    │   ────────────────────────────────────────
    │   QASearchPayload {
    │       project_id: "my-project",
    │       query: "如何配置 Rust logger?",
    │       limit: 6,
    │       min_score: 0.2,
    │   }
    │
    └─→ Call memory.search(payload)
            ↓

┌─────────────────────────────────────────────────────────────────┐
│  2. Plugin Layer (plugins/src/memory/service.rs)                │
└─────────────────────────────────────────────────────────────────┘
MemoryServicePlugin::search(payload)
    ↓
self.client.search(payload)  ← 调用 MemoryClient (core 层)
    ↓

┌─────────────────────────────────────────────────────────────────┐
│  3. HTTP Client (core/src/memory/client.rs) ⚠️                  │
└─────────────────────────────────────────────────────────────────┘
MemoryClient::search(payload)
    ↓
url = "https://memory.internal/v1/qa/search"
    ↓
HTTP POST Request
    Headers: {
        Authorization: "Bearer <api_key>",
        Content-Type: "application/json"
    }
    Body: {
        "project_id": "my-project",
        "query": "如何配置 Rust logger?",
        "limit": 6,
        "min_score": 0.2
    }
    ↓

┌─────────────────────────────────────────────────────────────────┐
│  4. External Service (Memory Service HTTP API)                  │
└─────────────────────────────────────────────────────────────────┘
Database Query:
    SELECT * FROM qa_records
    WHERE project_id = 'my-project'
      AND embedding_similarity(query, 'logger配置') >= 0.2
    ORDER BY score DESC
    LIMIT 6
    ↓
HTTP Response:
    {
        "matches": [
            {
                "qa_id": "qa_001",
                "query": "Rust logger 怎么配置?",
                "answer": "使用 tracing crate...",
                "score": 0.87,
                "trust": 0.75,
                "validation_level": 2,
                "metadata": { ... }
            },
            ...
        ]
    }
    ↓

┌─────────────────────────────────────────────────────────────────┐
│  5. Parse Response (core/src/memory/parse.rs)                   │
└─────────────────────────────────────────────────────────────────┘
parse_search_matches(&json)
    ↓
Vec<SearchMatch> = [
    SearchMatch {
        qa_id: "qa_001",
        query: "Rust logger 怎么配置?",
        answer: "使用 tracing crate...",
        score: 0.87,
        trust: 0.75,
        validation_level: 2,
        freshness: 0.92,
        status: "verified",
        ...
    },
    ...
]
    ↓

┌─────────────────────────────────────────────────────────────────┐
│  6. Gatekeeper Inject (core/src/gatekeeper/evaluate.rs)         │
└─────────────────────────────────────────────────────────────────┘
prepare_inject_list(&matches)
    ↓
Filter & Sort:
    - Filter: validation_level >= 2, trust >= 0.4
    - Sort: by (validation_level, trust, score, freshness)
    ↓
Vec<InjectItem> = [
    InjectItem {
        qa_id: "qa_001",
        reference_text: "[QA:qa_001] 使用 tracing crate...",
    }
]
    ↓

┌─────────────────────────────────────────────────────────────────┐
│  7. Merge into Prompt (core/src/engine/pre.rs)                  │
└─────────────────────────────────────────────────────────────────┘
merged_query = "如何配置 Rust logger?\n\n参考答案:\n[QA:qa_001] 使用 tracing crate..."
    ↓
Return to Engine → Execute with Backend
```

### 3.2 Record 流程

```
┌─────────────────────────────────────────────────────────────────┐
│  1. Post-Run Phase (core/src/engine/post.rs)                    │
└─────────────────────────────────────────────────────────────────┘
post_run(&ctx, &run_result, &matches, shown_qa_ids, &user_query)
    ↓
Gatekeeper.evaluate()
    ↓
GatekeeperDecision {
    hit_refs: [{ qa_id: "qa_001", shown: true, used: true }],
    validate_plans: [{ qa_id: "qa_001", result: pass }],
    should_write_candidate: true,
}
    ↓
    ├─→ Record Hit
    │   ────────────────────────────────────────
    │   build_hit_payload(&decision)
    │       ↓
    │   QAHitsPayload {
    │       project_id: "my-project",
    │       references: [
    │           { qa_id: "qa_001", shown: true, used: true }
    │       ]
    │   }
    │       ↓
    │   memory.record_hit(payload)
    │       ↓
    │   HTTP POST /v1/qa/hit
    │       ↓
    │   UPDATE qa_records SET
    │       hit_count = hit_count + 1,
    │       use_count = use_count + 1
    │   WHERE qa_id = 'qa_001'
    │
    ├─→ Record Validation
    │   ────────────────────────────────────────
    │   build_validate_payloads(&decision)
    │       ↓
    │   [QAValidationPayload {
    │       project_id: "my-project",
    │       qa_id: "qa_001",
    │       result: pass,
    │       notes: Some("执行成功")
    │   }]
    │       ↓
    │   for v in payloads {
    │       memory.record_validation(v)
    │   }
    │       ↓
    │   HTTP POST /v1/qa/validate
    │       ↓
    │   UPDATE qa_records SET
    │       validation_level = validation_level + 1,
    │       consecutive_fail = 0,
    │       trust = trust + 0.05
    │   WHERE qa_id = 'qa_001'
    │
    └─→ Record Candidate
        ────────────────────────────────────────
        extract_candidates(&config, query, stdout, stderr, tool_events)
            ↓
        [CandidateDraft {
            query: "如何配置 Rust logger?",
            answer: "使用 env_logger crate...",
            context: "[tool_events + stdout]",
            confidence: 0.82,
            tags: ["rust", "logger", "configuration"]
        }]
            ↓
        build_candidate_payloads(&drafts)
            ↓
        [QACandidatePayload { ... }]
            ↓
        for c in payloads {
            memory.record_candidate(c)
        }
            ↓
        HTTP POST /v1/qa/candidates
            ↓
        INSERT INTO qa_records (
            project_id, query, answer, context,
            validation_level, trust, confidence, ...
        ) VALUES (
            'my-project',
            '如何配置 Rust logger?',
            '使用 env_logger crate...',
            '[...]',
            0,  -- candidate
            0.5,  -- initial trust
            0.82,  -- confidence
            ...
        )
```

---

## 4. 扩展指南

### 4.1 添加新的 Memory 实现（如 gRPC）

**步骤 1**: 定义 gRPC client（在 plugins）

```rust
// plugins/src/memory/grpc_client.rs
pub struct GrpcMemoryClient {
    client: tonic::Client<MemoryServiceClient>,
}

impl GrpcMemoryClient {
    pub async fn search(&self, payload: QASearchPayload) -> Result<Vec<SearchMatch>> {
        let request = tonic::Request::new(SearchRequest {
            project_id: payload.project_id,
            query: payload.query,
            // ...
        });

        let response = self.client.search(request).await?;
        // Parse protobuf response to SearchMatch
        Ok(...)
    }
}
```

**步骤 2**: 实现 MemoryPlugin trait

```rust
// plugins/src/memory/grpc.rs
pub struct GrpcMemoryPlugin {
    client: GrpcMemoryClient,
}

#[async_trait]
impl MemoryPlugin for GrpcMemoryPlugin {
    fn name(&self) -> &str {
        "grpc_memory"
    }

    async fn search(&self, payload: QASearchPayload) -> Result<Vec<SearchMatch>> {
        self.client.search(payload).await
    }

    // 实现其他方法...
}
```

**步骤 3**: 注册到 factory

```rust
// plugins/src/factory.rs
pub fn build_memory(cfg: &AppConfig) -> Option<Box<dyn MemoryPlugin>> {
    match &cfg.memory.provider {
        MemoryProvider::Service(svc_cfg) => {
            if svc_cfg.protocol == "grpc" {
                Some(Box::new(GrpcMemoryPlugin::new(...)))
            } else {
                Some(Box::new(MemoryServicePlugin::new(...)))
            }
        }
    }
}
```

**无需修改 Core**: 所有引擎代码（pre/post）无需任何改动

### 4.2 添加本地 SQLite Memory（示例）

```rust
// plugins/src/memory/sqlite.rs
pub struct SqliteMemoryPlugin {
    conn: rusqlite::Connection,
}

#[async_trait]
impl MemoryPlugin for SqliteMemoryPlugin {
    fn name(&self) -> &str {
        "sqlite_memory"
    }

    async fn search(&self, payload: QASearchPayload) -> Result<Vec<SearchMatch>> {
        let rows = self.conn.query_map(
            "SELECT * FROM qa WHERE project_id = ? AND score >= ?",
            params![payload.project_id, payload.min_score],
            |row| {
                Ok(SearchMatch {
                    qa_id: row.get(0)?,
                    query: row.get(1)?,
                    // ...
                })
            },
        )?;

        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // 实现其他方法...
}
```

**配置**:

```toml
[memory]
provider = "sqlite"
path = "~/.memex/memory.db"
```

---

## 5. 待改进项

### 5.1 移动 MemoryClient 到 Plugins

**当前问题**: `core/src/memory/client.rs` 应移到 `plugins/src/memory/http_client.rs`

**改进后架构**:

```
core/src/memory/
├── trait.rs          # MemoryPlugin trait
├── models.rs         # Payload DTOs
├── payloads.rs       # Payload builders
├── extract.rs        # Candidate extraction
└── parse.rs          # Response parsing

plugins/src/memory/
├── service.rs        # MemoryServicePlugin (整合实现)
└── http_client.rs    # HTTP client (从 core 移出)
```

**收益**:
- Core 无 HTTP 依赖
- 符合依赖倒置原则
- 更清晰的职责分离

**实施参考**: `docs/ARCHITECTURE_ANALYSIS.md` 问题 #1

### 5.2 文档完善

**待添加文档**:
- [ ] Memory Service API 规范（OpenAPI）
- [ ] SearchMatch 字段语义说明
- [ ] Validation Level 升级规则详解
- [ ] Trust 计算公式文档

---

## 6. 常见问题 (FAQ)

**Q1: 为什么 Payload 构建在 core 而不是 plugins？**

A: Payload 构建包含业务逻辑（如从 GatekeeperDecision 提取 hit_refs），属于核心业务层，不应与具体通信实现耦合。Plugins 只负责"如何发送"，Core 负责"发送什么"。

**Q2: 为什么 SearchMatch 解析在 core？**

A: 解析逻辑包含向后兼容性处理和字段验证，属于业务规则。将其放在 core 可以确保无论使用哪种 Memory 实现（HTTP/gRPC/SQLite），解析逻辑一致。

**Q3: MemoryClient 应该在 core 还是 plugins？**

A: **应该在 plugins**。当前实现是历史遗留问题，违反了分层原则。参见改进方案 5.1。

**Q4: 如何 mock MemoryPlugin 进行测试？**

A: 由于 MemoryPlugin 是 trait，可以轻松创建 mock 实现：

```rust
struct MockMemoryPlugin {
    search_results: Vec<SearchMatch>,
}

#[async_trait]
impl MemoryPlugin for MockMemoryPlugin {
    fn name(&self) -> &str { "mock" }

    async fn search(&self, _: QASearchPayload) -> Result<Vec<SearchMatch>> {
        Ok(self.search_results.clone())
    }

    // 其他方法返回 Ok(())
}
```

---

**文档版本**: v1.0
**最后更新**: 2026-01-10
**相关文档**:
- `ARCHITECTURE_ANALYSIS.md` - 架构问题分析
- `CLAUDE.md` - 项目概览
