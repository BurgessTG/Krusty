//! Smart Codebase Memory System
//!
//! Provides semantic code indexing and insight accumulation for deep codebase understanding.
//!
//! Key components:
//! - `parser` - AST-based code parsing via tree-sitter
//! - `embeddings` - Local embeddings via fastembed (bge-small-en-v1.5)
//! - `codebase` - Codebase entity CRUD operations
//! - `insights` - Insight storage and retrieval
//! - `indexer` - Orchestrates the indexing process
//! - `retrieval` - Semantic search over indexed symbols

pub mod codebase;
pub mod embeddings;
pub mod indexer;
pub mod insights;
pub mod parser;
pub mod retrieval;

pub use codebase::{Codebase, CodebaseStore};
pub use embeddings::EmbeddingEngine;
pub use indexer::{IndexPhase, IndexProgress, Indexer};
pub use insights::{CodebaseInsight, InsightStore, InsightType};
pub use parser::{ParsedSymbol, RustParser, SymbolType};
pub use retrieval::{SearchQuery, SearchResult, SemanticRetrieval};
