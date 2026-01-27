#!/usr/bin/env python3
"""
Memory Hit Hook for Claude Code
Triggers on: Stop
Purpose: Extract used QA IDs from transcript and record hits (HTTP Server Version)
"""

import sys
import json
import subprocess
import os
from pathlib import Path
import re
from datetime import datetime
from project_utils import get_project_id_from_cwd
from session_state import load_session_state
from http_client import HTTPClient, direct_cli_call


def log_debug(message):
    """Log debug message to file"""
    hook_dir = Path.home().joinpath(".memex", "logs")
    log_file = hook_dir.joinpath("memory-hit.log")
    try:
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        with open(log_file, "a", encoding="utf-8") as f:
            f.write(f"{timestamp} {message}\n")
    except:
        pass


def record_hit_with_fallback(
    session_id: str,
    project_id: str,
    used_qa_ids: list,
    shown_qa_ids: list
) -> bool:
    """
    记录知识命中，优先使用HTTP服务器，失败时降级到直接调用

    Args:
        session_id: 会话 ID
        project_id: 项目 ID
        used_qa_ids: 已使用的 QA ID 列表
        shown_qa_ids: 已展示的 QA ID 列表

    Returns:
        是否记录成功
    """
    # 方案 A: 尝试使用HTTP服务器
    try:
        log_debug("Attempting to use HTTP server for record-hit...")
        client = HTTPClient(session_id)

        response = client.record_hit(
            project_id=project_id,
            qa_ids=used_qa_ids,
            shown_ids=shown_qa_ids
        )

        if response.get("success"):
            log_debug("✓ Hits recorded via HTTP server")
            return True
        else:
            error = response.get("error", "Unknown error")
            log_debug(f"HTTP server returned error: {error}")
            # 继续尝试直接调用

    except Exception as e:
        log_debug(f"HTTP server unavailable: {e}")
        return False


def extract_used_qa_ids_from_transcript(transcript_path):
    """
    从 transcript 中提取 assistant 使用的 QA ID

    查找 HTML 注释标记：<!-- memex-qa:qa-xxx -->

    Args:
        transcript_path: transcript 文件路径

    Returns:
        使用的 QA ID 列表
    """
    used_ids = set()

    if not transcript_path or not os.path.exists(transcript_path):
        return []

    try:
        with open(transcript_path, 'r', encoding='utf-8') as f:
            for line in f:
                if not line.strip():
                    continue

                try:
                    event = json.loads(line)
                    event_type = event.get('type', '')

                    # 查找 assistant_message 事件
                    if event_type == 'assistant_message':
                        content = event.get('content', '')

                        if isinstance(content, str):
                            # 匹配 HTML 注释中的 QA ID
                            # <!-- memex-qa:abc123 --> or <!-- memex-qa:qa-abc123 -->
                            pattern = r'<!-- memex-qa:([a-zA-Z0-9-]+) -->'
                            matches = re.findall(pattern, content)
                            used_ids.update(matches)

                        elif isinstance(content, list):
                            # content 可能是 list 格式
                            for item in content:
                                if isinstance(item, dict) and item.get('type') == 'text':
                                    text = item.get('text', '')
                                    pattern = r'<!-- memex-qa:([a-zA-Z0-9-]+) -->'
                                    matches = re.findall(pattern, text)
                                    used_ids.update(matches)

                except json.JSONDecodeError:
                    continue

    except Exception as e:
        log_debug(f"Error reading transcript: {e}")
        return []

    return list(used_ids)


def main():
    try:
        # 读取 Hook 输入
        hook_input = json.loads(sys.stdin.read())

        transcript_path = hook_input.get("transcript_path", "")
        session_id = hook_input.get("session_id", "unknown")
        cwd = hook_input.get("cwd", os.getcwd())

        log_debug(f"=== Memory Hit ===")
        log_debug(f"Session: {session_id}")
        log_debug(f"Transcript: {transcript_path}")

        # 生成 project_id
        project_id = get_project_id_from_cwd(cwd)
        log_debug(f"Project ID: {project_id}")

        # 加载会话状态
        session_state = load_session_state(session_id)
        shown_qa_ids = session_state.get('shown_qa_ids', [])

        if not shown_qa_ids:
            log_debug("No shown_qa_ids in session state, skipping")
            sys.exit(0)

        log_debug(f"Shown QA IDs: {shown_qa_ids}")

        # 从 transcript 提取使用的 QA IDs
        used_qa_ids = extract_used_qa_ids_from_transcript(transcript_path)

        if not used_qa_ids:
            log_debug("No used QA IDs found in transcript")
            sys.exit(0)

        log_debug(f"Used QA IDs: {used_qa_ids}")

        # 记录命中（优先使用守护进程，失败时降级到直接调用）
        log_debug(f"Recording hits: shown={len(shown_qa_ids)}, used={len(used_qa_ids)}")

        success = record_hit_with_fallback(
            session_id=session_id,
            project_id=project_id,
            used_qa_ids=used_qa_ids,
            shown_qa_ids=shown_qa_ids
        )

        if success:
            log_debug("✓ Hits recorded successfully")
        else:
            log_debug("✗ Failed to record hits")

        log_debug("=== Memory Hit Complete ===")
        sys.exit(0)

    except Exception as e:
        log_debug(f"Error in memory-hit: {e}")
        import traceback
        log_debug(traceback.format_exc())
        sys.exit(0)


if __name__ == "__main__":
    main()
