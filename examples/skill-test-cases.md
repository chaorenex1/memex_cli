# SKILL 测试用例文档

## 概述

本文档包含 `ux-design-gemini` 和 `code-with-codex` 两个 SKILL 的实际执行测试用例。

---

## 1. ux-design-gemini 测试用例

### 1.1 基本信息

| 项目 | 内容 |
|------|------|
| SKILL 名称 | ux-design-gemini |
| Backend | Gemini |
| 用途 | UX 设计文档生成 |
| 输出类型 | Markdown 设计规格文档 |

### 1.2 TC-UX-001: 计算器 UI 设计

```bash
memex-cli run --backend gemini --stdin <<'EOF'
---TASK---
id: calculator-design
backend: gemini
model: gemini-2.5-flash
workdir: C:\Users\zarag\Documents\aduib-app\claude-test
timeout: 180
---CONTENT---
设计一个非常漂亮的网页计算器 UI，要求：

## 设计目标
- 现代简洁风格，视觉吸引力强
- 适合桌面和移动端

## 输出内容
请提供完整的设计规格文档，包括：
1. 视觉设计规格（配色、字体、圆角、阴影）
2. 布局结构（整体布局、显示屏、按钮网格）
3. 组件规格（数字按钮、运算符按钮、功能按钮）
4. 交互设计（hover、active 状态）
5. 响应式设计（桌面端、移动端）
---END---
EOF
```

**执行结果**: PASS
**输出**: 完整的计算器 UI 设计规格文档

---

## 2. code-with-codex 测试用例

### 2.1 基本信息

| 项目 | 内容 |
|------|------|
| SKILL 名称 | code-with-codex |
| Backend | Codex |
| 用途 | 代码生成 |
| 输出类型 | 实际代码文件 |

### 2.2 模型选择指南

| 模型 | 适用场景 | 复杂度 |
|------|----------|--------|
| gpt-5.1-codex-mini | 简单脚本、快速修复 | * |
| gpt-5.2-codex | 通用编码、工具函数 | ** |
| gpt-5.1-codex-max | 平衡质量/速度 | *** |
| gpt-5.2 | 复杂逻辑、算法 | **** |

### 2.3 TC-CODE-001: 并行 DAG 四则运算

```bash
memex-cli run --backend codex --stdin <<'EOF'
---TASK---
id: add1
backend: codex
model: gpt-5.1-codex-mini
workdir: C:\Users\zarag\Documents\aduib-app\claude-test
---CONTENT---
计算 10 + 5，只输出数字结果
---END---

---TASK---
id: sub1
backend: codex
model: gpt-5.1-codex-mini
workdir: C:\Users\zarag\Documents\aduib-app\claude-test
---CONTENT---
计算 20 - 8，只输出数字结果
---END---

---TASK---
id: add2
backend: codex
model: gpt-5.1-codex-mini
workdir: C:\Users\zarag\Documents\aduib-app\claude-test
---CONTENT---
计算 3 + 1，只输出数字结果
---END---

---TASK---
id: mul1
backend: codex
model: gpt-5.1-codex-mini
workdir: C:\Users\zarag\Documents\aduib-app\claude-test
dependencies: add1, sub1
---CONTENT---
计算 add1 结果 * sub1 结果 (15 * 12)，只输出数字结果
---END---

---TASK---
id: div1
backend: codex
model: gpt-5.1-codex-mini
workdir: C:\Users\zarag\Documents\aduib-app\claude-test
dependencies: mul1, add2
---CONTENT---
计算 mul1 结果 / add2 结果 (180 / 4)，只输出数字结果
---END---
EOF
```

**DAG 结构**:
```
Stage 0: add1(10+5=15), sub1(20-8=12), add2(3+1=4)  [并行]
Stage 1: mul1(15*12=180)                            [依赖 add1, sub1]
Stage 2: div1(180/4=45)                             [依赖 mul1, add2]
```

**执行结果**: PASS
**最终结果**: 45
**耗时**: 70.7s (5 tasks, 3 stages)

---

### 2.4 TC-CODE-002: 计算器 HTML 生成

```bash
memex-cli run --backend codex --stdin <<'EOF'
---TASK---
id: calculator-html
backend: codex
model: gpt-5.2-codex
workdir: C:\Users\zarag\Documents\aduib-app\claude-test
timeout: 180
---CONTENT---
根据设计规格生成完整的网页计算器 HTML 文件（内嵌 CSS 和 JavaScript）

配色方案:
- 主背景: #202124
- 数字按钮: #3C4043
- 运算符按钮: #8AB4F8
- 文本: #E8EAED

功能要求:
- 四则运算 (+, -, *, /)
- 清除 (C)、正负切换 (±)、百分比 (%)
- 除零错误处理
- 响应式设计
---END---
EOF
```

**执行结果**: PASS
**输出**: calculator.html (8.5 KB, 329 lines)

---

## 3. 工作流集成测试

### TC-WORKFLOW-001: UX 设计 -> 代码生成

```
Step 1: ux-design-gemini
        输入: "设计计算器 UI"
        输出: 设计规格文档 (Markdown)
             ↓
Step 2: code-with-codex
        输入: 设计规格文档
        输出: calculator.html (实际代码)
```

**执行结果**: PASS
**验证**: 代码符合设计规格

---

## 4. 测试结果汇总

| 测试用例 | SKILL | 状态 |
|----------|-------|------|
| TC-UX-001 | ux-design-gemini | PASS |
| TC-CODE-001 | code-with-codex | PASS |
| TC-CODE-002 | code-with-codex | PASS |
| TC-WORKFLOW-001 | 集成 | PASS |

**通过率**: 100% (4/4)

---

## 5. 注意事项

### 5.1 ux-design-gemini

- 输出是**设计文档**，不是代码
- 支持图片分析（files-mode: embed）
- 适合生成用户流程、线框图规格、组件规范

### 5.2 code-with-codex

- 输出是**实际代码文件**
- 根据复杂度选择模型（mini -> codex -> max -> gpt-5.2）
- 支持 DAG 依赖和并行执行
- Windows 环境中文编码可能有问题，复杂内容建议用 Claude 直接写入

### 5.3 工作流建议

```
UX 需求 → ux-design-gemini → 设计规格 → code-with-codex → 代码文件
```

---

## 报告信息

| 项目 | 内容 |
|------|------|
| 生成时间 | 2026-01-12 |
| 生成者 | Claude Opus 4.5 |
| 文件名 | skill-test-cases.md |
