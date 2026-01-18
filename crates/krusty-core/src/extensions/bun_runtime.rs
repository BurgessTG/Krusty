//! Bun runtime management for extensions
//!
//! Replaces Node.js with Bun for faster JavaScript runtime.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{
    env::{self, consts},
    ffi::OsString,
    path::{Path, PathBuf},
    process::Output,
    sync::Arc,
};
use tokio::{fs, sync::RwLock};
use tracing::{debug, info, warn};

const BUN_VERSION: &str = "1.1.42";

#[cfg(not(windows))]
const BUN_BINARY: &str = "bun";
#[cfg(windows)]
const BUN_BINARY: &str = "bun.exe";

/// Bun runtime manager
#[derive(Clone)]
pub struct BunRuntime {
    inner: Arc<RwLock<BunRuntimeInner>>,
    http_client: reqwest::Client,
    data_dir: PathBuf,
}

struct BunRuntimeInner {
    instance: Option<BunInstance>,
}

#[derive(Clone, Debug)]
enum BunInstance {
    System { bun: PathBuf },
    Managed { installation_path: PathBuf },
}

impl BunRuntime {
    pub fn new(http_client: reqwest::Client, data_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(RwLock::new(BunRuntimeInner { instance: None })),
            http_client,
            data_dir,
        }
    }

    async fn get_instance(&self) -> Result<BunInstance> {
        // Check cache first
        {
            let inner = self.inner.read().await;
            if let Some(ref instance) = inner.instance {
                return Ok(instance.clone());
            }
        }

        // Try system Bun first
        match self.detect_system_bun().await {
            Ok(instance) => {
                info!("Using system Bun: {:?}", instance);
                let mut inner = self.inner.write().await;
                inner.instance = Some(instance.clone());
                return Ok(instance);
            }
            Err(e) => {
                debug!("System Bun not available: {}", e);
            }
        }

        // Fall back to managed Bun
        let instance = self.install_managed_bun().await?;
        info!("Using managed Bun: {:?}", instance);
        let mut inner = self.inner.write().await;
        inner.instance = Some(instance.clone());
        Ok(instance)
    }

    async fn detect_system_bun(&self) -> Result<BunInstance> {
        let bun = which::which("bun").context("bun not found in PATH")?;

        // Verify it works
        let output = tokio::process::Command::new(&bun)
            .arg("--version")
            .output()
            .await
            .context("failed to run bun --version")?;

        if !output.status.success() {
            bail!("bun --version failed");
        }

        let version_str = String::from_utf8_lossy(&output.stdout);
        info!("Found system Bun version: {}", version_str.trim());

        Ok(BunInstance::System { bun })
    }

    async fn install_managed_bun(&self) -> Result<BunInstance> {
        let os = match consts::OS {
            "macos" => "darwin",
            "linux" => "linux",
            "windows" => "windows",
            other => bail!("Unsupported OS: {}", other),
        };

        let arch = match consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "aarch64",
            other => bail!("Unsupported architecture: {}", other),
        };

        let bun_dir = self.data_dir.join("bun");
        let bun_binary = bun_dir.join(BUN_BINARY);

        // Check if already installed
        if fs::metadata(&bun_binary).await.is_ok() {
            let output = tokio::process::Command::new(&bun_binary)
                .arg("--version")
                .output()
                .await;

            if let Ok(output) = output {
                if output.status.success() {
                    return Ok(BunInstance::Managed {
                        installation_path: bun_dir,
                    });
                }
            }
            warn!("Existing Bun installation invalid, reinstalling");
        }

        // Download and install
        info!("Downloading Bun {}...", BUN_VERSION);

        // Bun uses different naming: bun-{os}-{arch}.zip
        let archive_name = if consts::OS == "windows" {
            format!("bun-windows-{}.zip", arch)
        } else {
            format!("bun-{}-{}.zip", os, arch)
        };

        let url = format!(
            "https://github.com/oven-sh/bun/releases/download/bun-v{}/{}",
            BUN_VERSION, archive_name
        );

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("failed to download Bun")?;

        if !response.status().is_success() {
            bail!("Failed to download Bun: {}", response.status());
        }

        let bytes = response.bytes().await.context("failed to read response")?;

        // Create directory
        fs::create_dir_all(&bun_dir).await?;

        // Extract zip
        self.extract_zip(&bytes, &bun_dir).await?;

        // Bun extracts to bun-{os}-{arch}/bun, need to move it up
        let extracted_dir = if consts::OS == "windows" {
            bun_dir.join(format!("bun-windows-{}", arch))
        } else {
            bun_dir.join(format!("bun-{}-{}", os, arch))
        };

        if extracted_dir.exists() {
            let extracted_binary = extracted_dir.join(BUN_BINARY);
            if extracted_binary.exists() {
                fs::rename(&extracted_binary, &bun_binary).await?;
            }
            // Clean up extracted directory
            let _ = fs::remove_dir_all(&extracted_dir).await;
        }

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&bun_binary).await?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&bun_binary, perms).await?;
        }

        info!("Bun installed to {}", bun_dir.display());

        Ok(BunInstance::Managed {
            installation_path: bun_dir,
        })
    }

    async fn extract_zip(&self, bytes: &[u8], dest: &Path) -> Result<()> {
        use std::io::Cursor;

        let reader = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader).context("failed to open Bun zip archive")?;
        archive.extract(dest).context("failed to extract Bun zip")?;
        Ok(())
    }

    /// Get path to bun binary
    pub async fn binary_path(&self) -> Result<PathBuf> {
        match self.get_instance().await? {
            BunInstance::System { bun } => Ok(bun),
            BunInstance::Managed { installation_path } => Ok(installation_path.join(BUN_BINARY)),
        }
    }

    /// Run a bun subcommand (replaces npm)
    pub async fn run_bun_subcommand(
        &self,
        directory: Option<&Path>,
        subcommand: &str,
        args: &[&str],
    ) -> Result<Output> {
        let instance = self.get_instance().await?;

        let bun_binary = match &instance {
            BunInstance::System { bun } => bun.clone(),
            BunInstance::Managed { installation_path } => installation_path.join(BUN_BINARY),
        };

        let env_path = path_with_bun_prepended(&bun_binary);

        let mut command = tokio::process::Command::new(&bun_binary);
        if let Some(path) = env_path {
            command.env("PATH", path);
        }
        command.arg(subcommand);
        command.args(args);

        if let Some(dir) = directory {
            command.current_dir(dir);
        }

        let output = command.output().await.context("failed to run bun")?;

        if !output.status.success() {
            bail!(
                "bun {} failed:\nstdout: {}\nstderr: {}",
                subcommand,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(output)
    }

    /// Get latest version of an npm package (bun compatible)
    pub async fn npm_package_latest_version(&self, package: &str) -> Result<String> {
        // Use bun's npm registry compatibility
        let output = self
            .run_bun_subcommand(None, "pm", &["info", package, "--json"])
            .await?;

        let info: BunPackageInfo = serde_json::from_slice(&output.stdout)
            .context("failed to parse bun pm info response")?;

        info.version
            .or(info.dist_tags.and_then(|dt| dt.latest))
            .with_context(|| format!("no version found for package {}", package))
    }

    /// Get installed version of a package
    pub async fn npm_package_installed_version(
        &self,
        directory: &Path,
        package: &str,
    ) -> Result<Option<String>> {
        let package_json = directory
            .join("node_modules")
            .join(package)
            .join("package.json");

        match fs::read_to_string(&package_json).await {
            Ok(content) => {
                let pkg: PackageJson = serde_json::from_str(&content)?;
                Ok(Some(pkg.version))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Install packages (uses bun install)
    pub async fn npm_install_packages(
        &self,
        directory: &Path,
        packages: &[(&str, &str)],
    ) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        let package_specs: Vec<String> = packages
            .iter()
            .map(|(name, version)| format!("{}@{}", name, version))
            .collect();

        let args: Vec<&str> = package_specs.iter().map(|s| s.as_str()).collect();

        self.run_bun_subcommand(Some(directory), "add", &args)
            .await?;
        Ok(())
    }
}

fn path_with_bun_prepended(bun_binary: &Path) -> Option<OsString> {
    let existing_path = env::var_os("PATH")?;
    let bun_dir = bun_binary.parent()?;

    env::join_paths(std::iter::once(bun_dir.to_path_buf()).chain(env::split_paths(&existing_path)))
        .ok()
}

#[derive(Deserialize)]
struct BunPackageInfo {
    version: Option<String>,
    #[serde(rename = "dist-tags")]
    dist_tags: Option<DistTags>,
}

#[derive(Deserialize)]
struct DistTags {
    latest: Option<String>,
}

#[derive(Deserialize)]
struct PackageJson {
    version: String,
}
