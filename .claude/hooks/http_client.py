#!/usr/bin/env python3
"""
HTTP客户端
用于hooks与HTTP服务器通信
"""
import os
import sys
import json
import subprocess
from typing import Dict, Any, Optional, List
from pathlib import Path

try:
    import httpx
    HTTPX_AVAILABLE = True
except ImportError:
    HTTPX_AVAILABLE = False

from server_manager import ServerManager


def get_utf8_env():
    """获取 UTF-8 编码的环境变量"""
    env = os.environ.copy()
    if os.name == 'nt':  # Windows
        env['PYTHONIOENCODING'] = 'utf-8'
    return env


class HTTPClient:
    """HTTP客户端"""

    def __init__(self, session_id: str):
        self.session_id = session_id
        self.server_manager = ServerManager(session_id)
        self.timeout = 30.0

        # 如果httpx可用，创建HTTP客户端
        if HTTPX_AVAILABLE:
            self._http_client = httpx.Client(timeout=self.timeout)
        else:
            self._http_client = None

    def __del__(self):
        """清理HTTP客户端"""
        if self._http_client:
            self._http_client.close()

    def _get_server_url(self) -> Optional[str]:
        """获取服务器URL"""
        return self.server_manager.get_server_url()

    def _send_request(
        self,
        method: str,
        endpoint: str,
        data: Dict = None
    ) -> Dict[str, Any]:
        """
        发送HTTP请求

        Args:
            method: HTTP方法
            endpoint: API端点
            data: 请求数据

        Returns:
            响应字典
        """
        server_url = self._get_server_url()

        if not server_url:
            return {"success": False, "error": "Server not running"}

        url = f"{server_url}{endpoint}"

        try:
            if self._http_client:
                # 使用httpx
                if method == "POST":
                    response = self._http_client.post(url, json=data)
                elif method == "GET":
                    response = self._http_client.get(url)
                else:
                    return {"success": False, "error": f"Unsupported method: {method}"}

                response.raise_for_status()
                return response.json()

            else:
                # 降级：使用urllib
                import urllib.request
                import urllib.error

                if method == "POST":
                    req = urllib.request.Request(
                        url,
                        data=json.dumps(data).encode('utf-8'),
                        headers={'Content-Type': 'application/json'}
                    )
                elif method == "GET":
                    req = urllib.request.Request(url)
                else:
                    return {"success": False, "error": f"Unsupported method: {method}"}

                with urllib.request.urlopen(req, timeout=self.timeout) as response:
                    return json.loads(response.read().decode('utf-8'))

        except Exception as e:
            return {"success": False, "error": f"HTTP request failed: {str(e)}"}

    def request(
        self,
        endpoint: str,
        method: str = "POST",
        data: Dict = None
    ) -> Dict[str, Any]:
        """
        通用HTTP请求接口

        Args:
            endpoint: API端点路径
            method: HTTP方法 (GET/POST)
            data: 请求数据

        Returns:
            响应字典
        """
        return self._send_request(method, endpoint, data)

    def search(
        self,
        query: str,
        project_id: str,
        limit: int = 5,
        min_score: float = 0.6
    ) -> Dict[str, Any]:
        """搜索记忆"""
        return self._send_request(
            "POST",
            "/api/v1/search",
            {
                "query": query,
                "project_id": project_id,
                "limit": limit,
                "min_score": min_score
            }
        )

    def record_candidate(
        self,
        project_id: str,
        question: str,
        answer: str
    ) -> Dict[str, Any]:
        """记录候选知识"""
        return self._send_request(
            "POST",
            "/api/v1/record-candidate",
            {
                "project_id": project_id,
                "question": question,
                "answer": answer
            }
        )

    def record_hit(
        self,
        project_id: str,
        qa_ids: List[str],
        shown_ids: List[str]
    ) -> Dict[str, Any]:
        """记录知识命中"""
        return self._send_request(
            "POST",
            "/api/v1/record-hit",
            {
                "project_id": project_id,
                "qa_ids": qa_ids,
                "shown_ids": shown_ids
            }
        )

    def record_validation(
        self,
        payload: Dict[str, Any]
    ) -> Dict[str, Any]:
        """记录知识验证"""
        return self._send_request(
            "POST",
            "/api/v1/record-validation",
            payload
        )

    def health_check(self) -> Dict[str, Any]:
        """健康检查"""
        return self._send_request("GET", "/health")

    def shutdown(self) -> Dict[str, Any]:
        """关闭服务器"""
        return self._send_request("POST", "/shutdown")


def direct_cli_call(command: str, args: Dict[str, Any]) -> Dict[str, Any]:
    """
    直接调用memex-cli（降级方案）

    Args:
        command: 命令名称
        args: 命令参数

    Returns:
        执行结果
    """
    cmd = ["memex-cli", command]

    for key, value in args.items():
        cmd.append(f"--{key}")
        cmd.append(str(value))

    # cmd.extend(["--format", "json"])

    try:
        env = get_utf8_env()

        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            encoding='utf-8',
            errors='replace',
            timeout=30,
            env=env
        )

        if result.returncode == 0:
            return {"success": True, "data": json.loads(result.stdout)}
        else:
            return {"success": False, "error": result.stderr}

    except Exception as e:
        return {"success": False, "error": str(e)}
