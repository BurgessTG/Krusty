//! Local embeddings via fastembed (bge-small-en-v1.5)
//!
//! Uses fastembed-rs for local embedding generation with the bge-small-en-v1.5 model.
//! Model specs: 384-dim vectors, ~33MB download.

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Embedding dimension for bge-small-en-v1.5
pub const EMBEDDING_DIM: usize = 384;

/// Size of embedding blob in bytes (384 f32s = 1536 bytes)
pub const EMBEDDING_BLOB_SIZE: usize = EMBEDDING_DIM * std::mem::size_of::<f32>();

/// Local embedding engine using fastembed
pub struct EmbeddingEngine {
    model: Arc<RwLock<TextEmbedding>>,
}

impl EmbeddingEngine {
    /// Create a new embedding engine (downloads model on first use)
    pub fn new() -> Result<Self> {
        info!("Initializing embedding engine (bge-small-en-v1.5)");

        let mut options = InitOptions::default();
        options.model_name = EmbeddingModel::BGESmallENV15;
        options.show_download_progress = true;

        let model =
            TextEmbedding::try_new(options).context("Failed to initialize embedding model")?;

        info!("Embedding engine ready");
        Ok(Self {
            model: Arc::new(RwLock::new(model)),
        })
    }

    /// Generate embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let model = self.model.read().await;
        let embeddings = model
            .embed(vec![text], None)
            .context("Failed to generate embedding")?;

        embeddings
            .into_iter()
            .next()
            .context("No embedding returned")
    }

    /// Generate embeddings for multiple texts (batched for efficiency)
    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        debug!(count = texts.len(), "Generating batch embeddings");
        let model = self.model.read().await;

        // Convert to slice of &str for the API
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        model
            .embed(text_refs, None)
            .context("Failed to generate batch embeddings")
    }

    /// Convert embedding vector to blob for database storage
    pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(EMBEDDING_BLOB_SIZE);
        for &value in embedding {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Convert blob from database to embedding vector
    pub fn blob_to_embedding(blob: &[u8]) -> Option<Vec<f32>> {
        if blob.len() != EMBEDDING_BLOB_SIZE {
            return None;
        }

        let mut embedding = Vec::with_capacity(EMBEDDING_DIM);
        for chunk in blob.chunks_exact(4) {
            let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            embedding.push(value);
        }
        Some(embedding)
    }

    /// Calculate cosine similarity between two embeddings
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }

    /// Find top-k most similar embeddings from a list
    pub fn top_k_similar(
        query: &[f32],
        candidates: &[(usize, Vec<f32>)],
        k: usize,
    ) -> Vec<(usize, f32)> {
        let mut scores: Vec<(usize, f32)> = candidates
            .iter()
            .map(|(id, emb)| (*id, Self::cosine_similarity(query, emb)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_roundtrip() {
        let embedding: Vec<f32> = (0..EMBEDDING_DIM).map(|i| i as f32 * 0.01).collect();
        let blob = EmbeddingEngine::embedding_to_blob(&embedding);
        let recovered = EmbeddingEngine::blob_to_embedding(&blob).unwrap();
        assert_eq!(embedding.len(), recovered.len());
        for (a, b) in embedding.iter().zip(recovered.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((EmbeddingEngine::cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!(EmbeddingEngine::cosine_similarity(&a, &c).abs() < 1e-6);
    }
}
