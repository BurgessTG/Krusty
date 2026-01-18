//! LSP transport layer - Content-Length framed JSON-RPC over stdio

use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;

/// Content-Length header prefix
const CONTENT_LENGTH: &str = "Content-Length: ";

/// LSP transport over stdio (async)
pub struct StdioTransport {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
}

impl StdioTransport {
    /// Create transport from a spawned child process
    /// Note: The child must be spawned with tokio::process::Command, not std::process::Command
    pub fn from_tokio_child(child: &mut tokio::process::Child) -> Result<Self> {
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("No stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;

        Ok(Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
        })
    }

    /// Send a JSON-RPC message
    pub async fn send(&self, message: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().await;

        // Write Content-Length header and body
        let header = format!("Content-Length: {}\r\n\r\n", message.len());
        stdin.write_all(header.as_bytes()).await?;
        stdin.write_all(message.as_bytes()).await?;
        stdin.flush().await?;

        Ok(())
    }

    /// Receive a JSON-RPC message
    pub async fn receive(&self) -> Result<String> {
        let mut stdout = self.stdout.lock().await;

        // Read headers until we find Content-Length, skipping non-LSP output
        // Some LSP servers (like golangci-lint) output log messages to stdout
        let mut content_length: Option<usize> = None;
        loop {
            let mut header = String::new();
            let bytes_read = stdout.read_line(&mut header).await?;
            if bytes_read == 0 {
                return Err(anyhow!("LSP server closed connection"));
            }

            let header = header.trim();

            // Skip empty lines when we don't have content-length yet
            if header.is_empty() {
                if content_length.is_some() {
                    // Empty line after Content-Length means end of headers
                    break;
                }
                // Otherwise skip empty lines (between messages or before first header)
                continue;
            }

            if let Some(len_str) = header.strip_prefix(CONTENT_LENGTH) {
                content_length = Some(len_str.parse()?);
            } else if header.starts_with("Content-") {
                // Other LSP headers like Content-Type - skip them
                continue;
            } else if content_length.is_none() {
                // Non-LSP output (log messages) - skip until we find Content-Length
                // This handles servers that print debug info to stdout
                continue;
            }
        }

        // Read body
        let length = content_length.ok_or_else(|| anyhow!("Missing Content-Length header"))?;
        let mut body = vec![0u8; length];
        stdout.read_exact(&mut body).await?;

        Ok(String::from_utf8(body)?)
    }
}
