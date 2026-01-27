//! Codebase entity CRUD operations

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A codebase entity representing an indexed project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Codebase {
    pub id: String,
    pub path: String,
    pub name: String,
    pub indexed_at: Option<DateTime<Utc>>,
    pub index_version: i32,
    pub config: CodebaseConfig,
}

/// Configuration for a codebase
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodebaseConfig {
    /// File patterns to include (e.g., ["**/*.rs"])
    #[serde(default)]
    pub include_patterns: Vec<String>,
    /// File patterns to exclude
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    /// Whether to index tests
    #[serde(default = "default_true")]
    pub index_tests: bool,
}

fn default_true() -> bool {
    true
}

/// Store for codebase CRUD operations
pub struct CodebaseStore<'a> {
    conn: &'a Connection,
}

impl<'a> CodebaseStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Create or update a codebase entry
    pub fn upsert(&self, codebase: &Codebase) -> Result<()> {
        let config_json =
            serde_json::to_string(&codebase.config).context("Failed to serialize config")?;
        let indexed_at = codebase.indexed_at.map(|dt| dt.to_rfc3339());

        self.conn.execute(
            "INSERT INTO codebases (id, path, name, indexed_at, index_version, config)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                name = excluded.name,
                indexed_at = excluded.indexed_at,
                index_version = excluded.index_version,
                config = excluded.config",
            params![
                codebase.id,
                codebase.path,
                codebase.name,
                indexed_at,
                codebase.index_version,
                config_json
            ],
        )?;

        Ok(())
    }

    /// Get a codebase by path
    pub fn get_by_path(&self, path: &str) -> Result<Option<Codebase>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, name, indexed_at, index_version, config FROM codebases WHERE path = ?1")?;

        let result = stmt.query_row([path], |row| {
            let id: String = row.get(0)?;
            let path: String = row.get(1)?;
            let name: String = row.get(2)?;
            let indexed_at: Option<String> = row.get(3)?;
            let index_version: i32 = row.get(4)?;
            let config_json: String = row.get(5)?;

            Ok((id, path, name, indexed_at, index_version, config_json))
        });

        match result {
            Ok((id, path, name, indexed_at, index_version, config_json)) => {
                let config: CodebaseConfig = serde_json::from_str(&config_json).unwrap_or_default();
                let indexed_at = indexed_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                });

                Ok(Some(Codebase {
                    id,
                    path,
                    name,
                    indexed_at,
                    index_version,
                    config,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get a codebase by ID
    pub fn get_by_id(&self, id: &str) -> Result<Option<Codebase>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, name, indexed_at, index_version, config FROM codebases WHERE id = ?1",
        )?;

        let result = stmt.query_row([id], |row| {
            let id: String = row.get(0)?;
            let path: String = row.get(1)?;
            let name: String = row.get(2)?;
            let indexed_at: Option<String> = row.get(3)?;
            let index_version: i32 = row.get(4)?;
            let config_json: String = row.get(5)?;

            Ok((id, path, name, indexed_at, index_version, config_json))
        });

        match result {
            Ok((id, path, name, indexed_at, index_version, config_json)) => {
                let config: CodebaseConfig = serde_json::from_str(&config_json).unwrap_or_default();
                let indexed_at = indexed_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                });

                Ok(Some(Codebase {
                    id,
                    path,
                    name,
                    indexed_at,
                    index_version,
                    config,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Create a new codebase from a path
    pub fn create_from_path(&self, path: &Path) -> Result<Codebase> {
        let path_str = path.to_string_lossy().to_string();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let codebase = Codebase {
            id: uuid::Uuid::new_v4().to_string(),
            path: path_str,
            name,
            indexed_at: None,
            index_version: 0,
            config: CodebaseConfig::default(),
        };

        self.upsert(&codebase)?;
        Ok(codebase)
    }

    /// Get or create a codebase for a path
    pub fn get_or_create(&self, path: &Path) -> Result<Codebase> {
        let path_str = path.to_string_lossy().to_string();
        if let Some(codebase) = self.get_by_path(&path_str)? {
            return Ok(codebase);
        }
        self.create_from_path(path)
    }

    /// Update the indexed timestamp
    pub fn mark_indexed(&self, id: &str, version: i32) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE codebases SET indexed_at = ?1, index_version = ?2 WHERE id = ?3",
            params![now, version, id],
        )?;
        Ok(())
    }

    /// Delete all index entries for a codebase
    pub fn clear_index(&self, codebase_id: &str) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM codebase_index WHERE codebase_id = ?1",
            [codebase_id],
        )?;
        Ok(deleted)
    }

    /// List all codebases
    pub fn list_all(&self) -> Result<Vec<Codebase>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, name, indexed_at, index_version, config FROM codebases ORDER BY name",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let path: String = row.get(1)?;
            let name: String = row.get(2)?;
            let indexed_at: Option<String> = row.get(3)?;
            let index_version: i32 = row.get(4)?;
            let config_json: String = row.get(5)?;

            Ok((id, path, name, indexed_at, index_version, config_json))
        })?;

        let mut codebases = Vec::new();
        for row in rows {
            let (id, path, name, indexed_at, index_version, config_json) = row?;
            let config: CodebaseConfig = serde_json::from_str(&config_json).unwrap_or_default();
            let indexed_at = indexed_at.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            });

            codebases.push(Codebase {
                id,
                path,
                name,
                indexed_at,
                index_version,
                config,
            });
        }

        Ok(codebases)
    }
}
