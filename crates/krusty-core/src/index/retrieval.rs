//! Semantic search over indexed symbols

use anyhow::Result;
use rusqlite::{params, Connection};

use super::embeddings::EmbeddingEngine;
use super::parser::SymbolType;

/// A search query
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// Text query for semantic search
    pub text: Option<String>,
    /// Filter by symbol type
    pub symbol_type: Option<SymbolType>,
    /// Filter by file path pattern
    pub file_pattern: Option<String>,
    /// Maximum results to return
    pub limit: usize,
}

impl SearchQuery {
    pub fn new() -> Self {
        Self {
            limit: 20,
            ..Default::default()
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn symbol_type(mut self, st: SymbolType) -> Self {
        self.symbol_type = Some(st);
        self
    }

    pub fn file_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.file_pattern = Some(pattern.into());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// A search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: i64,
    pub symbol_type: SymbolType,
    pub symbol_name: String,
    pub symbol_path: String,
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
    pub score: f32,
}

/// Semantic retrieval engine
pub struct SemanticRetrieval<'a> {
    conn: &'a Connection,
    embeddings: Option<&'a EmbeddingEngine>,
}

impl<'a> SemanticRetrieval<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self {
            conn,
            embeddings: None,
        }
    }

    pub fn with_embeddings(mut self, engine: &'a EmbeddingEngine) -> Self {
        self.embeddings = Some(engine);
        self
    }

    /// Search for symbols matching the query
    pub async fn search(&self, codebase_id: &str, query: SearchQuery) -> Result<Vec<SearchResult>> {
        // If we have a text query and embeddings, do semantic search
        if let (Some(text), Some(engine)) = (&query.text, self.embeddings) {
            return self
                .semantic_search(codebase_id, text, &query, engine)
                .await;
        }

        // Otherwise do keyword/filter search
        self.keyword_search(codebase_id, &query)
    }

    /// Semantic search using embeddings
    async fn semantic_search(
        &self,
        codebase_id: &str,
        text: &str,
        query: &SearchQuery,
        engine: &EmbeddingEngine,
    ) -> Result<Vec<SearchResult>> {
        // Generate query embedding
        let query_embedding = engine.embed(text).await?;

        // Load candidate embeddings from database
        let candidates = self.load_candidates(codebase_id, query)?;

        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate similarities
        let candidate_embeddings: Vec<(usize, Vec<f32>)> = candidates
            .iter()
            .enumerate()
            .filter_map(|(idx, (_, emb_opt, _))| emb_opt.clone().map(|e| (idx, e)))
            .collect();

        let scored =
            EmbeddingEngine::top_k_similar(&query_embedding, &candidate_embeddings, query.limit);

        // Build results
        let mut results = Vec::new();
        for (idx, score) in scored {
            if let Some((id, _, meta)) = candidates.get(idx) {
                results.push(SearchResult {
                    id: *id,
                    symbol_type: meta.symbol_type,
                    symbol_name: meta.symbol_name.clone(),
                    symbol_path: meta.symbol_path.clone(),
                    file_path: meta.file_path.clone(),
                    line_start: meta.line_start,
                    line_end: meta.line_end,
                    signature: meta.signature.clone(),
                    score,
                });
            }
        }

        Ok(results)
    }

    /// Load candidate symbols with embeddings
    fn load_candidates(
        &self,
        codebase_id: &str,
        query: &SearchQuery,
    ) -> Result<Vec<SearchCandidate>> {
        let mut sql = String::from(
            "SELECT id, symbol_type, symbol_name, symbol_path, file_path,
                    line_start, line_end, signature, embedding
             FROM codebase_index WHERE codebase_id = ?1",
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(codebase_id.to_string())];

        if let Some(st) = &query.symbol_type {
            sql.push_str(" AND symbol_type = ?");
            params_vec.push(Box::new(st.as_str().to_string()));
        }

        if let Some(pattern) = &query.file_pattern {
            sql.push_str(" AND file_path LIKE ?");
            params_vec.push(Box::new(format!("%{}%", pattern)));
        }

        let mut stmt = self.conn.prepare(&sql)?;

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let id: i64 = row.get(0)?;
            let symbol_type_str: String = row.get(1)?;
            let symbol_name: String = row.get(2)?;
            let symbol_path: String = row.get(3)?;
            let file_path: String = row.get(4)?;
            let line_start: i64 = row.get(5)?;
            let line_end: i64 = row.get(6)?;
            let signature: Option<String> = row.get(7)?;
            let embedding_blob: Option<Vec<u8>> = row.get(8)?;

            Ok((
                id,
                symbol_type_str,
                symbol_name,
                symbol_path,
                file_path,
                line_start,
                line_end,
                signature,
                embedding_blob,
            ))
        })?;

        let mut candidates = Vec::new();
        for row in rows {
            let (
                id,
                symbol_type_str,
                symbol_name,
                symbol_path,
                file_path,
                line_start,
                line_end,
                signature,
                embedding_blob,
            ) = row?;

            let symbol_type = SymbolType::parse(&symbol_type_str).unwrap_or(SymbolType::Function);
            let embedding = embedding_blob.and_then(|b| EmbeddingEngine::blob_to_embedding(&b));

            candidates.push((
                id,
                embedding,
                SymbolMeta {
                    symbol_type,
                    symbol_name,
                    symbol_path,
                    file_path,
                    line_start: line_start as usize,
                    line_end: line_end as usize,
                    signature,
                },
            ));
        }

        Ok(candidates)
    }

    /// Simple keyword search
    fn keyword_search(&self, codebase_id: &str, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let mut sql = String::from(
            "SELECT id, symbol_type, symbol_name, symbol_path, file_path,
                    line_start, line_end, signature
             FROM codebase_index WHERE codebase_id = ?1",
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(codebase_id.to_string())];

        if let Some(text) = &query.text {
            sql.push_str(" AND (symbol_name LIKE ? OR symbol_path LIKE ?)");
            let pattern = format!("%{}%", text);
            params_vec.push(Box::new(pattern.clone()));
            params_vec.push(Box::new(pattern));
        }

        if let Some(st) = &query.symbol_type {
            sql.push_str(" AND symbol_type = ?");
            params_vec.push(Box::new(st.as_str().to_string()));
        }

        if let Some(pattern) = &query.file_pattern {
            sql.push_str(" AND file_path LIKE ?");
            params_vec.push(Box::new(format!("%{}%", pattern)));
        }

        sql.push_str(&format!(" ORDER BY symbol_name LIMIT {}", query.limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let id: i64 = row.get(0)?;
            let symbol_type_str: String = row.get(1)?;
            let symbol_name: String = row.get(2)?;
            let symbol_path: String = row.get(3)?;
            let file_path: String = row.get(4)?;
            let line_start: i64 = row.get(5)?;
            let line_end: i64 = row.get(6)?;
            let signature: Option<String> = row.get(7)?;

            Ok((
                id,
                symbol_type_str,
                symbol_name,
                symbol_path,
                file_path,
                line_start,
                line_end,
                signature,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (
                id,
                symbol_type_str,
                symbol_name,
                symbol_path,
                file_path,
                line_start,
                line_end,
                signature,
            ) = row?;

            let symbol_type = SymbolType::parse(&symbol_type_str).unwrap_or(SymbolType::Function);

            results.push(SearchResult {
                id,
                symbol_type,
                symbol_name,
                symbol_path,
                file_path,
                line_start: line_start as usize,
                line_end: line_end as usize,
                signature,
                score: 1.0, // Keyword match gets full score
            });
        }

        Ok(results)
    }

    /// Get a specific symbol by ID
    pub fn get_symbol(&self, symbol_id: i64) -> Result<Option<SearchResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, symbol_type, symbol_name, symbol_path, file_path,
                    line_start, line_end, signature
             FROM codebase_index WHERE id = ?1",
        )?;

        let result = stmt.query_row([symbol_id], |row| {
            let id: i64 = row.get(0)?;
            let symbol_type_str: String = row.get(1)?;
            let symbol_name: String = row.get(2)?;
            let symbol_path: String = row.get(3)?;
            let file_path: String = row.get(4)?;
            let line_start: i64 = row.get(5)?;
            let line_end: i64 = row.get(6)?;
            let signature: Option<String> = row.get(7)?;

            Ok((
                id,
                symbol_type_str,
                symbol_name,
                symbol_path,
                file_path,
                line_start,
                line_end,
                signature,
            ))
        });

        match result {
            Ok((
                id,
                symbol_type_str,
                symbol_name,
                symbol_path,
                file_path,
                line_start,
                line_end,
                signature,
            )) => {
                let symbol_type =
                    SymbolType::parse(&symbol_type_str).unwrap_or(SymbolType::Function);

                Ok(Some(SearchResult {
                    id,
                    symbol_type,
                    symbol_name,
                    symbol_path,
                    file_path,
                    line_start: line_start as usize,
                    line_end: line_end as usize,
                    signature,
                    score: 1.0,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find symbols that call a given symbol
    pub fn find_callers(&self, codebase_id: &str, symbol_name: &str) -> Result<Vec<SearchResult>> {
        let pattern = format!("%\"{}%", symbol_name);

        let mut stmt = self.conn.prepare(
            "SELECT id, symbol_type, symbol_name, symbol_path, file_path,
                    line_start, line_end, signature
             FROM codebase_index WHERE codebase_id = ?1 AND calls LIKE ?2",
        )?;

        let rows = stmt.query_map(params![codebase_id, pattern], |row| {
            let id: i64 = row.get(0)?;
            let symbol_type_str: String = row.get(1)?;
            let symbol_name: String = row.get(2)?;
            let symbol_path: String = row.get(3)?;
            let file_path: String = row.get(4)?;
            let line_start: i64 = row.get(5)?;
            let line_end: i64 = row.get(6)?;
            let signature: Option<String> = row.get(7)?;

            Ok((
                id,
                symbol_type_str,
                symbol_name,
                symbol_path,
                file_path,
                line_start,
                line_end,
                signature,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (
                id,
                symbol_type_str,
                symbol_name,
                symbol_path,
                file_path,
                line_start,
                line_end,
                signature,
            ) = row?;

            let symbol_type = SymbolType::parse(&symbol_type_str).unwrap_or(SymbolType::Function);

            results.push(SearchResult {
                id,
                symbol_type,
                symbol_name,
                symbol_path,
                file_path,
                line_start: line_start as usize,
                line_end: line_end as usize,
                signature,
                score: 1.0,
            });
        }

        Ok(results)
    }
}

type SearchCandidate = (i64, Option<Vec<f32>>, SymbolMeta);

struct SymbolMeta {
    symbol_type: SymbolType,
    symbol_name: String,
    symbol_path: String,
    file_path: String,
    line_start: usize,
    line_end: usize,
    signature: Option<String>,
}
