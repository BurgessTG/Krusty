//! Insight storage and retrieval

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::embeddings::EmbeddingEngine;

/// Types of insights that can be accumulated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsightType {
    Architecture,
    Convention,
    Pitfall,
    BestPractice,
    Dependency,
    Performance,
}

impl InsightType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Architecture => "architecture",
            Self::Convention => "convention",
            Self::Pitfall => "pitfall",
            Self::BestPractice => "best_practice",
            Self::Dependency => "dependency",
            Self::Performance => "performance",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "architecture" => Some(Self::Architecture),
            "convention" => Some(Self::Convention),
            "pitfall" => Some(Self::Pitfall),
            "best_practice" => Some(Self::BestPractice),
            "dependency" => Some(Self::Dependency),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }

    /// Classify insight type from content (simple heuristic)
    pub fn classify(content: &str) -> Self {
        let lower = content.to_lowercase();

        if lower.contains("architecture")
            || lower.contains("module")
            || lower.contains("structure")
            || lower.contains("organization")
        {
            Self::Architecture
        } else if lower.contains("convention")
            || lower.contains("style")
            || lower.contains("naming")
            || lower.contains("format")
        {
            Self::Convention
        } else if lower.contains("avoid")
            || lower.contains("careful")
            || lower.contains("warning")
            || lower.contains("don't")
            || lower.contains("pitfall")
        {
            Self::Pitfall
        } else if lower.contains("performance")
            || lower.contains("slow")
            || lower.contains("optimize")
            || lower.contains("efficient")
        {
            Self::Performance
        } else if lower.contains("dependency")
            || lower.contains("import")
            || lower.contains("crate")
            || lower.contains("package")
        {
            Self::Dependency
        } else {
            Self::BestPractice
        }
    }
}

/// An accumulated insight about a codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseInsight {
    pub id: String,
    pub codebase_id: String,
    pub insight_type: InsightType,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub confidence: f64,
    pub source_session_id: Option<String>,
    pub access_count: i32,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
}

/// Store for insight operations
pub struct InsightStore<'a> {
    conn: &'a Connection,
}

impl<'a> InsightStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Create a new insight
    pub fn create(&self, insight: &CodebaseInsight) -> Result<()> {
        let embedding_blob = insight
            .embedding
            .as_ref()
            .map(|e| EmbeddingEngine::embedding_to_blob(e));

        self.conn.execute(
            "INSERT INTO codebase_insights
             (id, codebase_id, insight_type, content, embedding, confidence,
              source_session_id, access_count, created_at, last_accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                insight.id,
                insight.codebase_id,
                insight.insight_type.as_str(),
                insight.content,
                embedding_blob,
                insight.confidence,
                insight.source_session_id,
                insight.access_count,
                insight.created_at.to_rfc3339(),
                insight.last_accessed_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get insights for a codebase
    pub fn get_by_codebase(&self, codebase_id: &str) -> Result<Vec<CodebaseInsight>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, codebase_id, insight_type, content, embedding, confidence,
                    source_session_id, access_count, created_at, last_accessed_at
             FROM codebase_insights WHERE codebase_id = ?1
             ORDER BY confidence DESC, access_count DESC",
        )?;

        self.query_insights(&mut stmt, [codebase_id])
    }

    /// Get insights by type for a codebase
    pub fn get_by_type(
        &self,
        codebase_id: &str,
        insight_type: InsightType,
    ) -> Result<Vec<CodebaseInsight>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, codebase_id, insight_type, content, embedding, confidence,
                    source_session_id, access_count, created_at, last_accessed_at
             FROM codebase_insights WHERE codebase_id = ?1 AND insight_type = ?2
             ORDER BY confidence DESC, access_count DESC",
        )?;

        self.query_insights(&mut stmt, params![codebase_id, insight_type.as_str()])
    }

    /// Get top insights by confidence
    pub fn get_top(&self, codebase_id: &str, limit: usize) -> Result<Vec<CodebaseInsight>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, codebase_id, insight_type, content, embedding, confidence,
                    source_session_id, access_count, created_at, last_accessed_at
             FROM codebase_insights WHERE codebase_id = ?1
             ORDER BY confidence DESC, access_count DESC
             LIMIT ?2",
        )?;

        self.query_insights(&mut stmt, params![codebase_id, limit as i64])
    }

    /// Check if a similar insight already exists (by content similarity)
    pub fn has_similar(&self, codebase_id: &str, content: &str) -> Result<bool> {
        // Simple check: look for high text overlap
        let normalized = normalize_content(content);

        let mut stmt = self
            .conn
            .prepare("SELECT content FROM codebase_insights WHERE codebase_id = ?1 LIMIT 100")?;

        let rows = stmt.query_map([codebase_id], |row| row.get::<_, String>(0))?;

        for row in rows {
            let existing = row?;
            let existing_normalized = normalize_content(&existing);

            // Check for significant overlap using Jaccard similarity
            if jaccard_similarity(&normalized, &existing_normalized) > 0.7 {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn query_insights<P: rusqlite::Params>(
        &self,
        stmt: &mut rusqlite::Statement,
        params: P,
    ) -> Result<Vec<CodebaseInsight>> {
        let rows = stmt.query_map(params, |row| {
            let id: String = row.get(0)?;
            let codebase_id: String = row.get(1)?;
            let insight_type_str: String = row.get(2)?;
            let content: String = row.get(3)?;
            let embedding_blob: Option<Vec<u8>> = row.get(4)?;
            let confidence: f64 = row.get(5)?;
            let source_session_id: Option<String> = row.get(6)?;
            let access_count: i32 = row.get(7)?;
            let created_at: String = row.get(8)?;
            let last_accessed_at: String = row.get(9)?;

            Ok((
                id,
                codebase_id,
                insight_type_str,
                content,
                embedding_blob,
                confidence,
                source_session_id,
                access_count,
                created_at,
                last_accessed_at,
            ))
        })?;

        let mut insights = Vec::new();
        for row in rows {
            let (
                id,
                codebase_id,
                insight_type_str,
                content,
                embedding_blob,
                confidence,
                source_session_id,
                access_count,
                created_at,
                last_accessed_at,
            ) = row?;

            let insight_type =
                InsightType::parse(&insight_type_str).unwrap_or(InsightType::BestPractice);
            let embedding = embedding_blob.and_then(|b| EmbeddingEngine::blob_to_embedding(&b));
            let created_at = DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let last_accessed_at = DateTime::parse_from_rfc3339(&last_accessed_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            insights.push(CodebaseInsight {
                id,
                codebase_id,
                insight_type,
                content,
                embedding,
                confidence,
                source_session_id,
                access_count,
                created_at,
                last_accessed_at,
            });
        }

        Ok(insights)
    }
}

/// Normalize content for comparison
fn normalize_content(content: &str) -> Vec<String> {
    content
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .map(|s| s.to_string())
        .collect()
}

/// Calculate Jaccard similarity between two word sets
fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    use std::collections::HashSet;
    let set_a: HashSet<_> = a.iter().collect();
    let set_b: HashSet<_> = b.iter().collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Create a new insight from content
pub fn create_insight(
    codebase_id: &str,
    content: &str,
    session_id: Option<&str>,
    confidence: f64,
) -> CodebaseInsight {
    let now = Utc::now();
    CodebaseInsight {
        id: uuid::Uuid::new_v4().to_string(),
        codebase_id: codebase_id.to_string(),
        insight_type: InsightType::classify(content),
        content: content.to_string(),
        embedding: None,
        confidence,
        source_session_id: session_id.map(|s| s.to_string()),
        access_count: 0,
        created_at: now,
        last_accessed_at: now,
    }
}
