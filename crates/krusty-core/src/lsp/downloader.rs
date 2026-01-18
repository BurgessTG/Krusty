//! LSP binary downloader
//!
//! Handles downloading and installing LSP server binaries from:
//! - GitHub releases (rust-analyzer, zls, clangd, etc.)
//! - npm packages via Bun (pyright, typescript-language-server, etc.)
//! - Toolchain installers (gopls via go install, etc.)

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::PathBuf;
use tokio::fs;
use tokio::process::Command;
use tracing::{debug, info};

use super::builtin::{BuiltinLsp, LspInstallMethod};
use crate::paths::lsp_bin_dir;

/// LSP binary downloader and installer
pub struct LspDownloader {
    http_client: reqwest::Client,
    bin_dir: PathBuf,
}

impl LspDownloader {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
            bin_dir: lsp_bin_dir(),
        }
    }

    /// Ensure an LSP binary is available, downloading if needed
    /// Returns the path to the binary
    pub async fn ensure_available(&self, lsp: &BuiltinLsp) -> Result<PathBuf> {
        // First check if binary is in PATH
        if let Ok(path) = which::which(lsp.binary) {
            debug!("Found {} in PATH: {:?}", lsp.binary, path);
            return Ok(path);
        }

        // Check if binary exists in our managed bin dir
        let managed_bin = self.bin_dir.join(lsp.binary);
        if managed_bin.exists() {
            debug!("Found {} in managed dir: {:?}", lsp.binary, managed_bin);
            return Ok(managed_bin);
        }

        // Check environment variable to disable downloads
        if std::env::var("KRUSTY_DISABLE_LSP_DOWNLOAD").is_ok() {
            return Err(anyhow!(
                "LSP {} not found and KRUSTY_DISABLE_LSP_DOWNLOAD is set",
                lsp.id
            ));
        }

        // Ensure bin directory exists
        fs::create_dir_all(&self.bin_dir).await?;

        // Download based on install method
        match &lsp.install {
            LspInstallMethod::GitHub {
                repo,
                asset_pattern,
            } => self.download_github(lsp.binary, repo, asset_pattern).await,
            LspInstallMethod::Toolchain {
                toolchain,
                install_cmd,
            } => {
                self.install_toolchain(lsp.binary, toolchain, install_cmd)
                    .await
            }
            LspInstallMethod::Npm { package } => {
                self.install_npm(lsp.binary, package).await
            }
        }
    }

    /// Download from GitHub releases
    async fn download_github(
        &self,
        binary_name: &str,
        repo: &str,
        asset_pattern: &str,
    ) -> Result<PathBuf> {
        info!(
            "Downloading {} from GitHub releases ({})",
            binary_name, repo
        );

        // Fetch latest release info
        let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        let response = self
            .http_client
            .get(&url)
            .header("User-Agent", "krusty")
            .send()
            .await
            .context("Failed to fetch release info")?;

        if !response.status().is_success() {
            return Err(anyhow!("GitHub API error: {}", response.status()));
        }

        let release: serde_json::Value = response.json().await?;

        // Build asset name from pattern
        let (arch, platform, ext) = Self::platform_info();
        let asset_name = asset_pattern
            .replace("{arch}", arch)
            .replace("{platform}", platform)
            .replace("{ext}", ext);

        debug!("Looking for asset: {}", asset_name);

        // Find matching asset
        let assets = release["assets"]
            .as_array()
            .ok_or_else(|| anyhow!("No assets in release"))?;

        let asset = assets
            .iter()
            .find(|a| a["name"].as_str() == Some(&asset_name))
            .ok_or_else(|| anyhow!("Asset {} not found in release", asset_name))?;

        let download_url = asset["browser_download_url"]
            .as_str()
            .ok_or_else(|| anyhow!("No download URL for asset"))?;

        info!("Downloading from: {}", download_url);

        // Download the asset
        let response = self
            .http_client
            .get(download_url)
            .header("User-Agent", "krusty")
            .send()
            .await?;

        let bytes = response.bytes().await?;
        info!("Downloaded {} bytes", bytes.len());

        // Extract based on file extension
        let bin_path = if asset_name.ends_with(".gz") && !asset_name.ends_with(".tar.gz") {
            // Plain gzip (rust-analyzer style)
            self.extract_gz(&bytes, binary_name).await?
        } else if asset_name.ends_with(".tar.xz") {
            // tar.xz (zls style)
            self.extract_tar_xz(&bytes, binary_name).await?
        } else if asset_name.ends_with(".tar.gz") {
            // tar.gz (lua-language-server style)
            self.extract_tar_gz(&bytes, binary_name).await?
        } else if asset_name.ends_with(".zip") {
            // zip (clangd on windows style)
            self.extract_zip(&bytes, binary_name).await?
        } else {
            return Err(anyhow!("Unknown archive format: {}", asset_name));
        };

        // Set executable permission on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms)?;
        }

        info!("Installed {} to {:?}", binary_name, bin_path);
        Ok(bin_path)
    }

    /// Extract plain gzip file (single binary)
    async fn extract_gz(&self, bytes: &[u8], binary_name: &str) -> Result<PathBuf> {
        let mut decoder = GzDecoder::new(bytes);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;

        let bin_path = self.bin_dir.join(binary_name);
        fs::write(&bin_path, decompressed).await?;
        Ok(bin_path)
    }

    /// Extract tar.xz archive
    async fn extract_tar_xz(&self, bytes: &[u8], binary_name: &str) -> Result<PathBuf> {
        use std::io::Cursor;

        // Decompress xz
        let cursor = Cursor::new(bytes);
        let decoder = xz2::read::XzDecoder::new(cursor);

        // Extract tar
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&self.bin_dir)?;

        // The binary should now be in bin_dir
        let bin_path = self.bin_dir.join(binary_name);
        if bin_path.exists() {
            return Ok(bin_path);
        }

        // Some archives have the binary in a subdirectory
        Err(anyhow!("Binary {} not found after extraction", binary_name))
    }

    /// Extract tar.gz archive
    async fn extract_tar_gz(&self, bytes: &[u8], binary_name: &str) -> Result<PathBuf> {
        use std::io::Cursor;

        let cursor = Cursor::new(bytes);
        let decoder = GzDecoder::new(cursor);
        let mut archive = tar::Archive::new(decoder);

        // Create a subdirectory for this LSP
        let install_dir = self.bin_dir.join(format!("{}-install", binary_name));
        fs::create_dir_all(&install_dir).await?;

        archive.unpack(&install_dir)?;

        // Find the binary (might be in bin/ subdirectory)
        let direct = install_dir.join(binary_name);
        if direct.exists() {
            let target = self.bin_dir.join(binary_name);
            fs::rename(&direct, &target).await?;
            return Ok(target);
        }

        let in_bin = install_dir.join("bin").join(binary_name);
        if in_bin.exists() {
            let target = self.bin_dir.join(binary_name);
            fs::rename(&in_bin, &target).await?;
            return Ok(target);
        }

        Err(anyhow!("Binary {} not found in archive", binary_name))
    }

    /// Extract zip archive
    async fn extract_zip(&self, bytes: &[u8], binary_name: &str) -> Result<PathBuf> {
        use std::io::Cursor;

        let cursor = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;
        archive.extract(&self.bin_dir)?;

        let bin_path = self.bin_dir.join(binary_name);
        if bin_path.exists() {
            return Ok(bin_path);
        }

        // Check common subdirectory patterns
        let with_exe = self.bin_dir.join(format!("{}.exe", binary_name));
        if with_exe.exists() {
            return Ok(with_exe);
        }

        Err(anyhow!(
            "Binary {} not found after zip extraction",
            binary_name
        ))
    }

    /// Install via toolchain (go install, gem install, etc.)
    async fn install_toolchain(
        &self,
        binary_name: &str,
        toolchain: &str,
        install_cmd: &[&str],
    ) -> Result<PathBuf> {
        // Check if toolchain is available
        if which::which(toolchain).is_err() {
            return Err(anyhow!(
                "{} toolchain not found. Please install {} first.",
                toolchain,
                toolchain
            ));
        }

        info!("Installing {} via {}", binary_name, toolchain);

        let mut cmd = Command::new(install_cmd[0]);
        cmd.args(&install_cmd[1..]);

        // Set GOBIN for go install
        if toolchain == "go" {
            cmd.env("GOBIN", &self.bin_dir);
        }

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to install {}: {}", binary_name, stderr));
        }

        let bin_path = self.bin_dir.join(binary_name);
        if bin_path.exists() {
            info!("Installed {} to {:?}", binary_name, bin_path);
            return Ok(bin_path);
        }

        // On Windows, might have .exe extension
        let with_exe = self.bin_dir.join(format!("{}.exe", binary_name));
        if with_exe.exists() {
            return Ok(with_exe);
        }

        Err(anyhow!(
            "Binary {} not found after installation",
            binary_name
        ))
    }

    /// Install via npm/bun (preferred: bun for speed)
    async fn install_npm(&self, binary_name: &str, package: &str) -> Result<PathBuf> {
        // Prefer bun for speed, fall back to npm
        let (runner, install_args) = if which::which("bun").is_ok() {
            ("bun", vec!["install", "-g", "--cwd", self.bin_dir.to_str().unwrap_or("."), package])
        } else if which::which("npm").is_ok() {
            ("npm", vec!["install", "-g", "--prefix", self.bin_dir.to_str().unwrap_or("."), package])
        } else {
            return Err(anyhow!(
                "Neither bun nor npm found. Please install Node.js or Bun to install {}",
                package
            ));
        };

        info!("Installing {} via {} (package: {})", binary_name, runner, package);

        let output = Command::new(runner)
            .args(&install_args)
            .output()
            .await
            .context(format!("Failed to run {} install", runner))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "Failed to install {} via {}: {}",
                package,
                runner,
                stderr
            ));
        }

        // Check for binary in expected locations
        // npm with --prefix puts binaries in bin/ subdirectory
        let npm_bin = self.bin_dir.join("bin").join(binary_name);
        if npm_bin.exists() {
            info!("Installed {} to {:?}", binary_name, npm_bin);
            return Ok(npm_bin);
        }

        // bun might put it directly in the directory
        let direct = self.bin_dir.join(binary_name);
        if direct.exists() {
            info!("Installed {} to {:?}", binary_name, direct);
            return Ok(direct);
        }

        // Check global npm bin (fallback if prefix didn't work as expected)
        if let Ok(path) = which::which(binary_name) {
            info!("Found {} in PATH after install: {:?}", binary_name, path);
            return Ok(path);
        }

        Err(anyhow!(
            "Binary {} not found after npm install. You may need to run: {} install -g {}",
            binary_name,
            runner,
            package
        ))
    }

    /// Get platform info for asset pattern substitution
    fn platform_info() -> (&'static str, &'static str, &'static str) {
        let arch = std::env::consts::ARCH;

        let (platform, ext) = match std::env::consts::OS {
            "macos" => ("apple-darwin", "tar.xz"),
            "linux" => ("unknown-linux-gnu", "tar.xz"),
            "windows" => ("pc-windows-msvc", "zip"),
            other => (other, "tar.gz"),
        };

        (arch, platform, ext)
    }
}

impl Default for LspDownloader {
    fn default() -> Self {
        Self::new()
    }
}
