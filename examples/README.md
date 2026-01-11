# Memex-CLI Usage Examples

This directory contains practical examples demonstrating the structured text input feature.

## Overview

Memex-CLI supports two input modes:

- **Plain Text Mode** (`--no-structured-text`): For simple, single prompts
- **Structured Mode** (default): For multi-task workflows with dependencies

## Examples

### 1. Simple Prompt (Plain Text Mode)

**File**: `01_simple_prompt.txt`

**Use Case**: Quick, one-off prompts for code generation, explanations, or refactoring.

```bash
# From file
memex-cli run \
  --backend codex \
  --no-structured-text \
  --prompt-file examples/01_simple_prompt.txt

# Via stdin
cat examples/01_simple_prompt.txt | \
  memex-cli run --backend codex --no-structured-text --stdin

# Direct prompt
memex-cli run \
  --backend codex \
  --no-structured-text \
  --prompt "编写一个二分查找的 Rust 实现"
```

**When to Use**:
- Single question or request
- No dependencies on previous tasks
- Quick prototyping

---

### 2. Multi-Task Workflow (Structured Mode)

**File**: `02_multi_task_workflow.txt`

**Use Case**: Complete development workflow (design → implement → test → document).

```bash
# Execute the full workflow
memex-cli run --backend codex --stdin < examples/02_multi_task_workflow.txt

# Equivalent (--structured-text is default)
memex-cli run --backend codex --structured-text --stdin < examples/02_multi_task_workflow.txt
```

**Features Demonstrated**:
- ✅ Task dependencies (`dependencies: design-api`)
- ✅ Different backends per task (`claude` for design, `codex` for implementation)
- ✅ Model selection (`claude-sonnet-4`, `claude-opus-4`)
- ✅ Sequential execution based on dependency graph

**Execution Order**:
```
design-api (claude)
    ↓
implement-api (codex) ← depends on design-api
    ↓
write-tests (codex) ← depends on implement-api
    ↓
documentation (claude) ← depends on write-tests
```

---

### 3. Code Review Workflow (Structured Mode)

**File**: `03_code_review_workflow.txt`

**Use Case**: Multi-stage code review with different focus areas (security, performance, quality).

```bash
# Perform comprehensive code review
memex-cli run --backend claude --stdin < examples/03_code_review_workflow.txt
```

**Features Demonstrated**:
- ✅ File references (`files: src/auth.rs, src/api/user.rs`)
- ✅ File embedding (`files-mode: embed`)
- ✅ Sequential review stages with dependencies
- ✅ Final report generation

**Review Stages**:
1. **Security Review**: Check for vulnerabilities (Claude Opus 4)
2. **Performance Review**: Identify bottlenecks (Claude Sonnet 4)
3. **Code Quality Review**: Assess maintainability
4. **Generate Report**: Consolidate findings (Claude Opus 4)

---

## STDIO Protocol Format

### Basic Structure

```
---TASK---
id: task-identifier
backend: codex | claude | gemini
workdir: /path/to/working/directory
model: model-name (optional)
model-provider: provider-name (optional)
dependencies: task1, task2 (optional)
stream-format: text | jsonl (optional)
timeout: seconds (optional)
retry: count (optional)
files: file1.rs, file2.rs (optional)
files-mode: embed | ref | auto (optional)
files-encoding: utf-8 | base64 | auto (optional)
---CONTENT---
Your prompt or instruction here.
Can span multiple lines.
---END---
```

### Required Fields

- `id`: Unique task identifier (alphanumeric, underscores, hyphens, dots)
- `backend`: Backend to use (codex, claude, gemini, or custom URL)
- `workdir`: Working directory path

### Optional Fields

- `model`: Specific model to use (e.g., `claude-opus-4`, `gpt-4`)
- `model-provider`: Provider override (e.g., `openai`, `anthropic`)
- `dependencies`: Comma-separated list of task IDs that must complete first
- `stream-format`: Output format (`text` or `jsonl`)
- `timeout`: Maximum execution time in seconds
- `retry`: Number of retry attempts on failure
- `files`: Comma-separated list of files to include
- `files-mode`: How to include files (`embed`, `ref`, or `auto`)
- `files-encoding`: File encoding (`utf-8`, `base64`, or `auto`)

### Task Dependencies

Tasks are executed in topologically sorted order based on dependencies:

```
---TASK---
id: task-a
backend: codex
workdir: /project
---CONTENT---
Step A
---END---

---TASK---
id: task-b
backend: codex
workdir: /project
dependencies: task-a
---CONTENT---
Step B (depends on task-a)
---END---

---TASK---
id: task-c
backend: codex
workdir: /project
dependencies: task-a, task-b
---CONTENT---
Step C (depends on both task-a and task-b)
---END---
```

**Execution Order**: task-a → task-b → task-c

**Validation**:
- ✅ Circular dependencies are detected and rejected
- ✅ Unknown dependencies trigger errors
- ✅ Tasks can reference previous task outputs in their prompts

---

## Error Handling

### Invalid Structured Input

If you provide structured input with errors, you'll get helpful suggestions:

```bash
$ echo "plain text" | memex-cli run --backend codex --stdin
❌ Failed to parse structured text: No '---TASK---' marker found

💡 Tip: Use --no-structured-text to treat input as plain text

Example:
  memex-cli run --backend codex --no-structured-text --stdin <<< "plain text"
```

### Missing Required Fields

```bash
$ memex-cli run --backend codex --stdin <<EOF
---TASK---
backend: codex
---CONTENT---
test
---END---
EOF

❌ Failed to parse structured text: metadata missing required field 'id'

💡 Tip: Add the missing 'id' field or use --no-structured-text
```

### Circular Dependencies

```bash
❌ Failed to parse structured text: Circular dependency detected: task1 -> task2 -> task1

💡 Tip: Remove circular dependencies from your task definitions
```

---

## Tips & Best Practices

### 1. Choose the Right Mode

```bash
# Plain text: Single question
memex-cli run --backend codex --no-structured-text \
  --prompt "Explain Rust ownership"

# Structured: Multi-step workflow
memex-cli run --backend codex --stdin < workflow.txt
```

### 2. Use Descriptive Task IDs

```
Good: design-api, implement-auth, write-tests
Bad: task1, task2, task3
```

### 3. Leverage Different Models

```
---TASK---
id: design
backend: claude
model: claude-opus-4    # Use Opus for creative/design tasks
---CONTENT---
Design the architecture
---END---

---TASK---
id: implement
backend: codex          # Use Codex for implementation
dependencies: design
---CONTENT---
Implement the design
---END---
```

### 4. File References

```
---TASK---
id: review-code
backend: claude
files: src/*.rs, tests/*.rs
files-mode: embed
---CONTENT---
Review the above code files for security issues
---END---
```

### 5. Error Recovery with Retry

```
---TASK---
id: flaky-task
backend: codex
retry: 3
timeout: 300
---CONTENT---
Task that might fail occasionally
---END---
```

---

## Advanced Usage

### Combining with Shell Scripts

```bash
#!/bin/bash
# generate_api.sh

# Create structured input dynamically
cat > tasks.txt <<EOF
---TASK---
id: generate-api
backend: codex
workdir: $(pwd)
---CONTENT---
Generate REST API for $1 resource
---END---
EOF

memex-cli run --backend codex --stdin < tasks.txt
```

### Pipeline Integration

```bash
# Use memex-cli in a pipeline
git diff HEAD~1 | \
  memex-cli run --backend claude \
    --no-structured-text \
    --stdin \
    --prompt "Summarize these code changes"
```

### CI/CD Integration

```yaml
# .github/workflows/ai-review.yml
name: AI Code Review
on: [pull_request]

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: AI Code Review
        run: |
          memex-cli run \
            --backend claude \
            --model claude-opus-4 \
            --no-structured-text \
            --prompt "Review this PR for security issues" \
            --prompt-file <(git diff ${{ github.event.pull_request.base.sha }})
```

---

## Troubleshooting

### Problem: "Failed to parse structured text"

**Solution**: Use `--no-structured-text` for plain prompts:
```bash
memex-cli run --backend codex --no-structured-text --prompt "your prompt"
```

### Problem: Circular dependency error

**Solution**: Review your dependency graph and remove cycles:
```
task-a depends on task-b
task-b depends on task-a  ← Remove this
```

### Problem: Task output not visible

**Solution**: Check `--stream-format`:
```bash
# For human-readable output
memex-cli run --backend codex --stdin < tasks.txt

# For machine-parseable output
memex-cli run --backend codex --stdin < tasks.txt --stream-format jsonl
```

---

## More Examples

For additional examples, see:
- [STDIO Protocol Documentation](../docs/STDIO_PROTOCOL.md)
- [Structured Text Design](../docs/STRUCTURED_TEXT_DESIGN.md)
- [Integration Tests](../core/tests/structured_text_integration.rs)

## Feedback

If you have questions or encounter issues:
- Open an issue: https://github.com/chaorenex/memex-cli/issues
- Check documentation: `memex-cli run --help`
