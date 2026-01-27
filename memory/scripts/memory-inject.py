#!/usr/bin/env python3
"""
Memory Inject Hook for Claude Code
Triggers on: UserPromptSubmit
Purpose: Search memory service and inject relevant context (HTTP Server Version)
"""

import sys
import json
import subprocess
import os
from pathlib import Path
from datetime import datetime
from project_utils import get_project_id_from_cwd
from session_state import update_session_state
from http_client import HTTPClient, direct_cli_call


def log_debug(message):
    """Log debug message to file"""
    hook_dir = Path.home().joinpath(".memex", "logs")
    log_file = hook_dir.joinpath("memory-inject.log")
    try:
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        with open(log_file, "a", encoding="utf-8") as f:
            f.write(f"{timestamp} {message}\n")
    except:
        pass


def search_memory_with_fallback(
    session_id: str,
    query: str,
    project_id: str,
    limit: int = 5,
    min_score: float = 0.6
):
    """
    搜索记忆，优先使用HTTP服务器，失败时降级到直接调用

    Args:
        session_id: 会话 ID
        query: 搜索查询
        project_id: 项目 ID
        limit: 最大结果数
        min_score: 最低相关性分数

    Returns:
        搜索结果字典，如果失败返回 None
    """
    # 方案 A: 尝试使用HTTP服务器
    try:
        log_debug("Attempting to use HTTP server for search...")
        client = HTTPClient(session_id)

        response = client.search(
            query=query,
            project_id=project_id,
            limit=limit,
            min_score=min_score
        )

        if response.get("success"):
            log_debug("✓ Search via HTTP server succeeded")
            return response.get("data", {})
        else:
            error = response.get("error", "Unknown error")
            log_debug(f"HTTP server returned error: {error}")

    except Exception as e:
        log_debug(f"HTTP server unavailable: {e}")


def main():
    try:
        # 读取 Hook 输入
        hook_input = json.loads(sys.stdin.read())
        log_debug(f"Hook triggered: {json.dumps(hook_input, ensure_ascii=False)[:200]}")

        user_prompt = hook_input.get("prompt", "")
        log_debug(f"User Prompt: {user_prompt}")
        cwd = hook_input.get("cwd", os.getcwd())
        session_id = hook_input.get("session_id", "unknown")

        # 生成 project_id
        project_id = get_project_id_from_cwd(cwd)
        log_debug(f"Project ID: {project_id}")

        # 搜索记忆（优先使用 HTTP 服务器，失败时降级到直接调用）
        search_result = search_memory_with_fallback(
            session_id=session_id,
            query=user_prompt,
            project_id=project_id,
            limit=10,
            min_score=0.6
        )

        if search_result is None:
            log_debug("Search failed with both daemon and direct call")
            sys.exit(0)
        if len(search_result) == 0:
            log_debug("Search returned empty result")
            sys.exit(0)
        log_debug(f"Search Result: {json.dumps(search_result, ensure_ascii=False)}")

        # Normalize response shape
        # - HTTP server: { merged_query, shown_qa_ids, matches }
        # - direct-cli fallback (legacy): [matches...]
        if isinstance(search_result, list):
            merged_query = user_prompt
            matches = search_result
            shown_qa_ids = [m.get("qa_id", "") for m in matches if isinstance(m, dict) and m.get("qa_id")]
        else:
            merged_query = search_result.get("merged_query") or user_prompt
            matches = search_result.get("matches", [])
            shown_qa_ids = search_result.get("shown_qa_ids")
            if not isinstance(shown_qa_ids, list) or not shown_qa_ids:
                shown_qa_ids = [m.get("qa_id", "") for m in matches if isinstance(m, dict) and m.get("qa_id")]

        if not matches:
            log_debug("No matches found")
            sys.exit(0)

        # shown_qa_ids may come from server response; fallback derived above

        # 格式化为 Markdown 上下文（使用 HTML 注释标记 QA ID）
        context_lines = [
            "### 📚 相关历史记忆\n",
            "以下是从记忆系统中检索到的相关知识，优先使用相关性高的内容。\n",
            "**重要**：如果你使用了某条知识，必须在回答中保留其 HTML 注释标记（`<!-- memex-qa:ID -->`），以便追踪知识使用情况。\n",
            "**使用规则**：",
            "- 优先使用相关性评分高的知识",
            "- 如果知识不相关，可以忽略",
            "- 使用知识时保持其 HTML 注释标记不变",
            "- 不要编造不存在的知识\n"
        ]

        for match in matches:
            qa_id = match.get("qa_id", "unknown")
            question = match.get("question", "")
            answer = match.get("answer", "")
            score = match.get("score", 0.0)

            # 使用 HTML 注释标记（不可见）
            context_lines.append(f"<!-- memex-qa:{qa_id} -->")
            context_lines.append(f"**Q**: {question}")
            context_lines.append(f"**A**: {answer}")
            context_lines.append(f"_相关性: {score:.2f}_")
            context_lines.append(f"<!-- /memex-qa -->\n---\n")

        additional_context = "\n".join(context_lines)
        log_debug(f"Injecting {len(matches)} matches with QA IDs: {shown_qa_ids}")

        # 保存到会话状态（供 Stop Hook 使用：evaluate-session 需要 matches / merged_query / shown_qa_ids）
        update_session_state(session_id, {
            "shown_qa_ids": shown_qa_ids,
            "query": user_prompt,
            "merged_query": merged_query,
            "matches": matches,
            "project_id": project_id,
            "updated_at": datetime.now().isoformat(timespec="seconds")
        })
        log_debug(f"Saved shown_qa_ids to session state")

        # 输出 Hook 响应
        response = {
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": additional_context
            },
            "continue": True,
            "suppressOutput": False
        }

        print(json.dumps(response, ensure_ascii=False))
        log_debug("Memory inject completed successfully")
        sys.exit(0)

    except subprocess.TimeoutExpired:
        log_debug("Search timeout")
        sys.exit(0)
    except Exception as e:
        log_debug(f"Unexpected error: {e}")
        import traceback
        log_debug(traceback.format_exc())
        sys.exit(0)


if __name__ == "__main__":
    main()
