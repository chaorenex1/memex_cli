//! 远程客户端 - 通过 HTTP 调用 Core Server

use anyhow::Result;
use futures::stream::StreamExt;
use memex_core::api as core_api;
use reqwest::Client;
use serde_json::json;
use std::io::{self, Write};

/// 远程客户端
#[derive(Clone)]
pub struct RemoteClient {
    client: Client,
    server_url: String,
}

impl RemoteClient {
    /// 创建新的远程客户端
    pub fn new(server_url: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .build()
            .unwrap();

        Self { client, server_url }
    }

    /// 从配置创建客户端
    pub fn from_config(server_url: &str) -> Self {
        Self::new(server_url.to_string())
    }

    /// 执行 run 命令
    pub async fn exec_run(
        &self,
        tasks: &Vec<core_api::StdioTask>,
        stdio_opts: &core_api::StdioRunOpts,
    ) -> Result<i32, core_api::RunnerError> {
        let payload = json!({
            "tasks": tasks,
            "options": stdio_opts,
            "sse": true,
        });
        self.exec_command("run", &payload).await
    }

    /// 执行 replay 命令
    pub async fn exec_replay(
        &self,
        args: memex_core::api::ReplayArgs,
    ) -> Result<i32, core_api::RunnerError> {
        let payload = json!({
            "events": args.events,
            "run_id": args.run_id,
            "format": args.format,
            "set": args.set,
            "rerun_gatekeeper": args.rerun_gatekeeper,
        });

        self.exec_command("replay", &payload).await
    }

    /// 执行 resume 命令
    pub async fn exec_resume(&self, run_id: &str) -> Result<i32, core_api::RunnerError> {
        let payload = json!({
            "run_id": run_id
        });

        self.exec_command("resume", &payload).await
    }

    /// 通用命令执行
    async fn exec_command(
        &self,
        command: &str,
        payload: &serde_json::Value,
    ) -> Result<i32, core_api::RunnerError> {
        let url = format!("{}/exec/{}", self.server_url, command);

        tracing::debug!(target: "memex.client", "Sending {} request to {}", command, url);

        let response = self
            .client
            .post(&url)
            .json(payload)
            .send()
            .await
            .map_err(|e| core_api::RunnerError::Spawn(format!("Failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(core_api::RunnerError::Spawn(format!(
                "Request failed with status {}: {}",
                status, error_text
            )));
        }

        // 获取响应流并直接输出到 stdout
        let mut stdout = io::stdout().lock();
        let mut stream = response.bytes_stream();

        let mut exit_code = 0;
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result
                .map_err(|e| core_api::RunnerError::Spawn(format!("Stream error: {}", e)))?;

            // 检查是否是退出码标记
            let chunk_str = String::from_utf8_lossy(&chunk);
            if let Some(exit_str) = chunk_str.strip_prefix("[Exit: ") {
                if let Some(end) = exit_str.strip_suffix("]") {
                    if let Ok(code) = end.trim().parse::<i32>() {
                        exit_code = code;
                    }
                }
            } else {
                // 直接输出
                stdout.write_all(&chunk).map_err(|e| {
                    core_api::RunnerError::Spawn(format!("Failed to write to stdout: {}", e))
                })?;
                stdout.flush().map_err(|e| {
                    core_api::RunnerError::Spawn(format!("Failed to flush stdout: {}", e))
                })?;
            }
        }

        Ok(exit_code)
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<bool, core_api::RunnerError> {
        let url = format!("{}/health", self.server_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| core_api::RunnerError::Spawn(format!("Health check failed: {}", e)))?;

        Ok(response.status().is_success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_client_new() {
        let client = RemoteClient::new("http://localhost:8080".to_string());
        assert_eq!(client.server_url, "http://localhost:8080");
    }

    #[test]
    fn test_remote_client_from_config() {
        let client = RemoteClient::from_config("http://127.0.0.1:9090");
        assert_eq!(client.server_url, "http://127.0.0.1:9090");
    }
}
