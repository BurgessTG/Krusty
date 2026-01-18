//! MCP stdio transport
//!
//! Simple stdio transport for local MCP servers. Uses newline-delimited JSON.
//! Each message is a JSON object followed by a newline.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

/// Stdio transport for MCP servers
pub struct StdioTransport {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    child: Mutex<Child>,
}

impl StdioTransport {
    /// Spawn an MCP server process
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: &Path,
    ) -> Result<Self> {
        tracing::info!("Spawning MCP server: {} {:?}", command, args);
        for (k, v) in env {
            // Mask API keys in logs
            let masked = if k.contains("API_KEY") || k.contains("TOKEN") {
                format!("{}...{}", &v.chars().take(8).collect::<String>(), &v.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>())
            } else {
                v.clone()
            };
            tracing::info!("  env {}={}", k, masked);
        }

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(working_dir)
            .kill_on_drop(true);

        for (key, value) in env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow!("Command not found: {}. Is it installed and in PATH?", command)
            } else {
                anyhow!("Failed to spawn {}: {}", command, e)
            }
        })?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("No stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;

        Ok(Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            child: Mutex::new(child),
        })
    }

    /// Send a JSON-RPC message (newline-delimited JSON)
    pub async fn send(&self, message: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(message.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        tracing::debug!("Sent: {}", message);
        Ok(())
    }

    /// Receive a JSON-RPC message (newline-delimited JSON)
    pub async fn receive(&self) -> Result<String> {
        let mut stdout = self.stdout.lock().await;

        loop {
            let mut line = String::new();
            let bytes = stdout.read_line(&mut line).await?;

            if bytes == 0 {
                // EOF - check if process died
                let mut child = self.child.lock().await;
                match child.try_wait() {
                    Ok(Some(status)) => {
                        return Err(anyhow!("MCP server exited with {}", status));
                    }
                    Ok(None) => {
                        return Err(anyhow!("MCP server closed stdout unexpectedly"));
                    }
                    Err(e) => {
                        return Err(anyhow!("Error checking MCP server status: {}", e));
                    }
                }
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Validate it's JSON
            if line.starts_with('{') {
                tracing::debug!("Received: {}", line);
                return Ok(line.to_string());
            }

            // Skip non-JSON lines (could be debug output from server)
            tracing::debug!("Skipping non-JSON line: {}", line);
        }
    }

    /// Check if process is still running
    pub async fn is_alive(&self) -> bool {
        let mut child = self.child.lock().await;
        matches!(child.try_wait(), Ok(None))
    }
}
