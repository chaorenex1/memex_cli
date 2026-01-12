#!/usr/bin/env python3
"""
Project ID Utilities for Memex Hooks

提供从 cwd 参数自动生成 project_id 的功能。
零配置，完全自包含，无外部依赖。
"""

import os
import hashlib


def get_project_id_from_cwd(cwd: str) -> str:
    """
    从 cwd 生成 project_id

    格式: 规范化路径字符串（纯路径格式）
    示例: c--users-user-projects-memex_cli

    规则：
    1. 转小写
    2. Windows驱动器 "C:\\" → "c--"
    3. Unix路径移除开头 "/"
    4. 路径分隔符 / 和 \\ → "-"
    5. 空格和特殊字符 → "_"
    6. 最大长度 64 字符

    Args:
        cwd: Hook 输入中的 cwd 字段（当前工作目录）

    Returns:
        project_id 字符串，格式为规范化路径

    Examples:
        >>> get_project_id_from_cwd("C:\\Users\\user\\projects\\memex_cli")
        'c--users-user-projects-memex_cli'

        >>> get_project_id_from_cwd("/home/user/projects/my-app")
        'home-user-projects-my-app'
    """
    if not cwd:
        return "default"

    # 规范化路径（处理 Windows/Linux 差异）
    normalized = os.path.normpath(cwd).lower()

    # Windows: 驱动器号处理 "c:\\" → "c--"
    drive_letter = None
    if len(normalized) >= 2 and normalized[1] == ':':
        drive_letter = normalized[0]
        rest_path = normalized[3:] if len(normalized) > 3 else ""  # 跳过 ":\\" 或 ":/"
        # 统一路径分隔符为 "-"
        rest_path = rest_path.replace('\\', '-').replace('/', '-')
        # 直接处理剩余路径，稍后拼接驱动器号
        normalized = rest_path

    # Unix: 移除开头的 "/"
    elif normalized.startswith('/'):
        normalized = normalized[1:]
        # 统一路径分隔符为 "-"
        normalized = normalized.replace('/', '-')

    else:
        # 相对路径
        normalized = normalized.replace('\\', '-').replace('/', '-')

    # 清理非法字符
    result = _sanitize_project_id(normalized)

    # 添加驱动器前缀
    if drive_letter:
        result = f"{drive_letter}--{result}" if result else f"{drive_letter}--"

    return result


def _sanitize_project_id(raw_id: str) -> str:
    """
    清理 project_id，确保符合规范

    规则：
    - 只保留字母、数字、连字符、下划线
    - 转换为小写
    - 移除连续的特殊字符
    - 限制长度为 64 字符

    Args:
        raw_id: 原始 project_id

    Returns:
        清理后的 project_id
    """
    if not raw_id:
        return "default"

    # 替换非字母数字字符为下划线（保留 - 和 _）
    sanitized = ''.join(
        c if c.isalnum() or c in '-_' else '_'
        for c in raw_id
    )

    # 转小写
    sanitized = sanitized.lower()

    # 移除连续的下划线和连字符
    while '__' in sanitized:
        sanitized = sanitized.replace('__', '_')
    while '--' in sanitized:
        sanitized = sanitized.replace('--', '-')

    # 去除首尾的下划线和连字符
    sanitized = sanitized.strip('_-')

    # 限制长度（最多 64 字符）
    if len(sanitized) > 64:
        sanitized = sanitized[:64]

    return sanitized if sanitized else "default"


# 示例用法和测试
if __name__ == "__main__":
    # 测试用例
    test_cases = [
        "C:\\Users\\zarag\\Documents\\aduib-app\\memex_cli",
        "/home/user/projects/my-app",
        "/var/www/html",
        "D:\\Code\\Projects\\Test Project",
        "",
        "project-with-special-chars!@#$%",
    ]

    print("=== Project ID Generation Test ===\n")
    for cwd in test_cases:
        project_id = get_project_id_from_cwd(cwd)
        print(f"CWD: {cwd or '(empty)'}")
        print(f"Project ID: {project_id}")
        print()
