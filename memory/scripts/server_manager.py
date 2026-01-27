#!/usr/bin/env python3
"""
HTTP服务器管理器 - 管理 Rust memex-cli HTTP 服务器生命周期
"""
import os
import sys
import time
import subprocess
import json
import signal
import traceback
import socket
from pathlib import Path
from typing import Optional, Dict


class ServerManager:
    """HTTP服务器管理器（Rust memex-cli）"""

    def __init__(self, session_id: str):
        self.session_id = session_id
        self.servers_dir = Path.home() / ".memex" / "servers"
        self.servers_dir.mkdir(parents=True, exist_ok=True)

        self.state_file = self.servers_dir / "memex.state"
        self.log_file = self.servers_dir / "memex.log"

    @staticmethod
    def _debug(message: str):
        if os.environ.get("MEMEX_HOOK_DEBUG") == "1":
            print(message, file=sys.stderr)

    def load_state(self) -> Optional[Dict]:
        """加载服务器状态"""
        if not self.state_file.exists():
            return None

        try:
            with open(self.state_file, 'r', encoding='utf-8') as f:
                return json.load(f)
        except (json.JSONDecodeError, IOError):
            return None

    @staticmethod
    def _is_process_alive(pid: int) -> bool:
        """检查进程是否存活"""
        try:
            if os.name == 'nt':  # Windows
                result = subprocess.run(
                    ["tasklist", "/FI", f"PID eq {pid}"],
                    capture_output=True,
                    text=True,
                    timeout=2
                )
                return str(pid) in result.stdout
            else:  # Unix
                os.kill(pid, 0)
                return True
        except (OSError, subprocess.TimeoutExpired):
            return False

    def is_server_running(self) -> bool:
        """检查服务器是否运行"""
        state = self.load_state()
        self._debug(f"Loaded server state: {state}")

        if not state:
            return False

        pid = state.get('pid')
        if not pid:
            return False

        return self._is_process_alive(pid)

    def start_server(self, wait_for_ready: bool = True, max_wait_seconds: float = 10.0) -> bool:
        """
        启动 Rust HTTP 服务器

        Args:
            wait_for_ready: 是否等待端口监听就绪；False 时只负责启动进程（非阻塞）
            max_wait_seconds: 等待就绪的最大时间（秒）

        Returns:
            是否启动成功
        """
        # 检查服务器是否已运行
        if self.is_server_running():
            return True

        # 获取端口和主机
        port = self.preferred_server_port()
        host = self.get_hostname()

        # 构建启动命令
        command = [
            "memex-cli" if os.name != "nt" else "memex-cli.exe",
            "http-server",
        ]
        self._debug(f"Starting server with command: {command}")
        self._debug(f"Server logs will be written to: {self.log_file}")

        # 启动服务器
        try:
            # 打开日志文件（避免占用/权限问题导致 hook 失败；必要时降级到 DEVNULL）
            try:
                log_handle = open(self.log_file, 'a', encoding='utf-8')
            except Exception:
                log_handle = subprocess.DEVNULL

            # 跨平台启动
            if os.name == 'nt':  # Windows
                subprocess.Popen(
                    command,
                    creationflags=(
                        subprocess.DETACHED_PROCESS
                        | subprocess.CREATE_NEW_PROCESS_GROUP
                        | subprocess.CREATE_NO_WINDOW
                    ),
                    close_fds=True,
                    stdin=subprocess.DEVNULL,
                    stdout=log_handle,
                    stderr=log_handle
                )
            else:  # Unix
                subprocess.Popen(
                    command,
                    start_new_session=True,
                    close_fds=True,
                    stdin=subprocess.DEVNULL,
                    stdout=log_handle,
                    stderr=log_handle
                )

            # 非阻塞模式：仅启动进程，不等待就绪
            if not wait_for_ready:
                return True

            # 等待服务器启动（检查端口监听）
            sleep_seconds = 0.5
            max_retries = max(1, int(max_wait_seconds / sleep_seconds))
            for i in range(max_retries):
                time.sleep(sleep_seconds)

                # 检查状态文件是否存在
                if self.state_file.exists():
                    # 检查端口是否监听
                    if self._is_port_listening(port, host):
                        self._debug(f"Server started successfully on {host}:{port}")
                        return True

                # 每隔几次输出进度
                if i % 5 == 4:
                    self._debug(f"Waiting for server to start... ({i+1}/{max_retries})")

            # 启动超时，输出日志帮助诊断
            self._debug("Server startup timeout. Last 20 lines of log:")
            self._print_last_log_lines(20)
            return False

        except Exception as e:
            if os.environ.get("MEMEX_HOOK_DEBUG") == "1":
                traceback.print_exception(type(e), e, e.__traceback__, file=sys.stderr)
            self._debug(f"Failed to start server: {e}")
            self._debug("Last 20 lines of log:")
            self._print_last_log_lines(20)
            return False

    def stop_server(self, timeout: int = 10) -> bool:
        """
        停止HTTP服务器

        Args:
            timeout: 超时时间（秒）

        Returns:
            是否停止成功
        """
        state = self.load_state()

        if not state:
            return True

        pid = state.get('pid')
        if not pid:
            return True

        if not self._is_process_alive(pid):
            self._cleanup()
            return True

        try:
            # 优雅关闭
            if os.name == 'nt':  # Windows
                subprocess.run(
                    ["taskkill", "/F", "/PID", str(pid)],
                    capture_output=True,
                    timeout=timeout
                )
            else:  # Unix
                os.kill(pid, signal.SIGTERM)

                # 等待退出
                for _ in range(timeout * 10):
                    if not self._is_process_alive(pid):
                        break
                    time.sleep(0.1)

                # 强制kill
                if self._is_process_alive(pid):
                    os.kill(pid, signal.SIGKILL)
                    time.sleep(0.5)

            # 清理
            self._cleanup()
            return True

        except Exception as e:
            self._debug(f"Failed to stop server: {e}")
            return False

    def get_server_url(self) -> Optional[str]:
        """获取服务器URL"""
        state = self.load_state()
        return state.get('url') if state else None

    def get_server_port(self) -> Optional[int]:
        """获取服务器端口"""
        state = self.load_state()
        return state.get('port') if state else None

    @staticmethod
    def _is_port_available(port: int, host: str = "127.0.0.1") -> bool:
        """
        检查端口是否可用

        Args:
            port: 端口号
            host: 主机地址

        Returns:
            端口是否可用
        """
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                s.bind((host, port))
                return True
        except OSError:
            return False

    def preferred_server_port(self, start_port: int = 8001, max_attempts: int = 100) -> int:
        """
        推荐服务器端口（动态分配可用端口）

        Args:
            start_port: 起始端口（默认8000）
            max_attempts: 最大尝试次数（默认100）

        Returns:
            可用端口号
        """
        host = self.get_hostname()

        # 首先尝试从state文件获取已记录的端口
        state = self.load_state()
        if state and 'port' in state:
            recorded_port = int(state['port'])
            if self._is_port_available(recorded_port, host):
                self._debug(f"Reusing recorded port: {recorded_port}")
                return recorded_port

        # 从起始端口开始查找可用端口
        for offset in range(max_attempts):
            port = start_port + offset
            if self._is_port_available(port, host):
                self._debug(f"Found available port: {port}")
                return port

        # 如果所有端口都不可用，返回起始端口（让服务器启动时报错）
        self._debug(f"Warning: No available port found, falling back to {start_port}")
        return start_port

    @staticmethod
    def get_hostname() -> str:
        """
        获取主机IP地址

        Returns:
            主机IP地址（优先返回局域网IP，回退到127.0.0.1）
        """
        return "127.0.0.1"

    def _cleanup(self):
        """清理服务器状态文件"""
        if self.state_file.exists():
            self.state_file.unlink()

    def _is_port_listening(self, port: int, host: str = "127.0.0.1", timeout: float = 0.2) -> bool:
        """
        检查端口是否正在监听

        Args:
            port: 端口号
            host: 主机地址

        Returns:
            端口是否正在监听
        """
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.settimeout(timeout)
                result = s.connect_ex((host, port))
                return result == 0
        except Exception:
            return False

    def _print_last_log_lines(self, lines: int = 20):
        """
        输出日志文件的最后几行

        Args:
            lines: 要输出的行数
        """
        try:
            if not self.log_file.exists():
                self._debug("Log file does not exist")
                return

            with open(self.log_file, 'r', encoding='utf-8') as f:
                all_lines = f.readlines()
                last_lines = all_lines[-lines:] if len(all_lines) > lines else all_lines

                for line in last_lines:
                    self._debug(line.rstrip())
        except Exception as e:
            self._debug(f"Failed to read log file: {e}")
