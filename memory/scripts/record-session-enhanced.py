#!/usr/bin/env python3
"""
Enhanced Record Session Hook for Claude Code (v2 with Rust Gatekeeper)
Triggers on: Stop (session end)
Purpose: Full Memory lifecycle using Rust gatekeeper evaluate-session API
"""

import sys
import json
import os
from pathlib import Path
from datetime import datetime
from typing import Dict, Any, Optional

# Import our modules
from transcript_parser import parse_transcript, ParsedTranscript
from http_client import HTTPClient
from project_utils import get_project_id_from_cwd
from session_state import load_session_state


def log_debug(message: str):
    """Log debug message to file"""
    hook_dir = Path.home().joinpath(".memex", "logs")
    hook_dir.mkdir(parents=True, exist_ok=True)
    log_file = hook_dir.joinpath("record-session-enhanced.log")
    try:
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        with open(log_file, "a", encoding="utf-8") as f:
            f.write(f"{timestamp} {message}\n")
    except:
        pass


def evaluate_session_with_gatekeeper(
    client: HTTPClient,
    parsed: ParsedTranscript,
    project_id: str,
    transcript_path: str,
    state: Optional[Dict[str, Any]] = None
) -> Optional[Dict[str, Any]]:
    """
    Call Rust evaluate-session API with parsed transcript data

    Args:
        client: HTTP client instance
        parsed: Parsed transcript data
        project_id: Project ID

    Returns:
        API response dict or None if failed
    """
    try:
        state = state or {}

        # Prefer merged_query/matches from memory-inject hook (search_handler output)
        merged_query = state.get("merged_query")
        if not isinstance(merged_query, str) or not merged_query.strip():
            merged_query = parsed.user_query

        matches = state.get("matches", [])
        if not isinstance(matches, list):
            matches = []

        shown_qa_ids = state.get("shown_qa_ids")
        if not isinstance(shown_qa_ids, list) or not shown_qa_ids:
            shown_qa_ids = parsed.shown_qa_ids

        # Convert to Rust evaluate-session request format (transcript_path-based)
        request_data = {
            "project_id": project_id,
            "session_id": parsed.session_id,
            "user_query": merged_query,
            "matches": matches,
            "transcript_path": transcript_path,
            "stdout": parsed.stdout,
            "stderr": parsed.stderr,
            "shown_qa_ids": shown_qa_ids,
            "used_qa_ids": parsed.used_qa_ids,
            "exit_code": parsed.exit_code,
            "duration_ms": parsed.duration_ms
        }

        log_debug(
            "Calling evaluate-session API "
            f"(session_id={parsed.session_id}, matches={len(matches)}, shown_qa_ids={len(shown_qa_ids)})"
        )

        # Call the new evaluate-session endpoint
        # log_debug(f"Request data: {json.dumps(request_data)}")
        response = client.request(
            endpoint="/api/v1/evaluate-session",
            method="POST",
            data=request_data
        )

        if not response.get("success"):
            log_debug(f"Evaluate-session failed: {response}")
            return None

        log_debug(
            f"Evaluate-session scheduled: {response.get('decision_summary', 'N/A')}\n"
            f"  - Candidates recorded (immediate): {response.get('candidates_recorded', 0)}\n"
            f"  - Hits recorded (immediate): {response.get('hits_recorded', 0)}\n"
            f"  - Validations recorded (immediate): {response.get('validations_recorded', 0)}"
        )

        return response

    except Exception as e:
        log_debug(f"Failed to call evaluate-session: {e}")
        return None


def main():
    """Main execution"""
    try:
        # Read hook input
        hook_input = json.loads(sys.stdin.read())
        log_debug(f"Hook triggered: session_id={hook_input.get('session_id')}")

        session_id = hook_input.get("session_id", "unknown")
        transcript_path = hook_input.get("transcript_path", "")
        agent_transcript_path = hook_input.get("agent_transcript_path", "")
        if agent_transcript_path:
            log_debug(f"Agent transcript path found: {agent_transcript_path}")
            transcript_path = agent_transcript_path  # Prefer agent transcript if available
        cwd = hook_input.get("cwd", os.getcwd())

        if not transcript_path or not Path(transcript_path).exists():
            log_debug(f"Transcript not found: {transcript_path}")
            print(json.dumps({"success": False, "error": "Transcript not found"}))
            return

        # Get project ID
        project_id = get_project_id_from_cwd(cwd)
        log_debug(f"Project ID: {project_id}")

        # Load shared state from memory-inject (search results)
        try:
            session_state = load_session_state(session_id)
        except Exception:
            session_state = {}

        # Parse transcript
        log_debug("Parsing transcript...")
        parsed = parse_transcript(transcript_path, session_id)
        log_debug(
            f"Parsed: user_query_len={len(parsed.user_query)}, "
            f"tool_events={len(parsed.tool_events)}, "
            f"exit_code={parsed.exit_code}, "
            f"shown_qa_ids={len(parsed.shown_qa_ids)}, "
            f"used_qa_ids={len(parsed.used_qa_ids)}"
        )

        # Create HTTP client
        client = HTTPClient(session_id)

        # Call Rust evaluate-session API (now scheduled in background on server)
        result = evaluate_session_with_gatekeeper(
            client,
            parsed,
            project_id,
            transcript_path=transcript_path,
            state=session_state
        )

        if result:
            # Success - server accepted and scheduled evaluation
            output = {
                "success": True,
                "message": "Session evaluation scheduled",
                "gatekeeper_decision": result.get("decision_summary"),
                "candidates_recorded": result.get("candidates_recorded", 0),
                "hits_recorded": result.get("hits_recorded", 0),
                "validations_recorded": result.get("validations_recorded", 0)
            }
            log_debug("✅ Session evaluation scheduled successfully")
        else:
            # Fallback: just log that we tried
            output = {
                "success": False,
                "error": "Failed to evaluate session with gatekeeper"
            }
            log_debug("❌ Session evaluation failed")

        # Output result
        print(json.dumps(output, indent=2))

    except Exception as e:
        log_debug(f"Fatal error: {e}")
        print(json.dumps({"success": False, "error": str(e)}))


if __name__ == '__main__':
    main()
