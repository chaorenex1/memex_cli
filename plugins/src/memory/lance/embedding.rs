//! Embedding service for text vectorization.
//!
//! Supports both local (Ollama) and remote (OpenAI) embedding providers.

use anyhow::{Context, Result};
use async_trait::async_trait;

/// Trait for embedding text into vectors.
#[async_trait]
pub trait EmbeddingService: Send + Sync {
    /// Generate embedding for a single text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts (batch processing).
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Get the embedding dimension.
    fn dimension(&self) -> usize;
}

/// Ollama local embedding service.
pub struct OllamaEmbeddingService {
    client: reqwest::Client,
    base_url: String,
    model: String,
    dimension: usize,
}

impl OllamaEmbeddingService {
    /// Create a new Ollama embedding service.
    pub fn new(base_url: String, model: String, dimension: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            model,
            dimension,
        }
    }

    /// Create with default Ollama configuration.
    pub fn default_config() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "http://localhost:11434".to_string(),
            model: "nomic-embed-text".to_string(),
            dimension: 768,
        }
    }
}

#[async_trait]
impl EmbeddingService for OllamaEmbeddingService {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let request = OllamaEmbedRequest {
            model: self.model.clone(),
            input: text.to_string(),
            prompt: text.to_string(),
            dimensions: self.dimension,
            encoding_format: "float".to_string(),
        };

        let url = format!("{}/api/embeddings", self.base_url);
        tracing::debug!(
            "Sending embedding request to Ollama: url={}, model={}, text_len={}",
            url,
            self.model,
            text.len()
        );

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Failed to send embedding request to Ollama at {}. Is Ollama running?",
                    self.base_url
                )
            })?;

        let status = response.status();
        tracing::debug!("Ollama response status: {}", status);

        let result: OllamaEmbedResponse = response
            .error_for_status()
            .with_context(|| format!("Ollama returned error status: {}", status))?
            .json()
            .await
            .context("Failed to parse Ollama embedding response")?;

        tracing::debug!(
            "Ollama embedding received: dimension={}",
            result.embedding.len()
        );

        Ok(result.embedding)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        use futures::future::try_join_all;

        // Process embeddings concurrently for better performance
        // Limit concurrency to avoid overwhelming the embedding service
        let concurrent_limit = 8usize;
        let chunks = texts.chunks(concurrent_limit);

        let mut all_results = Vec::with_capacity(texts.len());

        for chunk in chunks {
            let futures = chunk.iter().map(|text| self.embed(text));
            let results = try_join_all(futures).await?;
            all_results.extend(results);
        }

        Ok(all_results)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[derive(serde::Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: String,
    prompt: String,
    dimensions: usize,
    encoding_format: String,
}

#[derive(serde::Deserialize)]
struct OllamaEmbedResponse {
    embedding: Vec<f32>,
}

/// OpenAI remote embedding service.
pub struct OpenAIEmbeddingService {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAIEmbeddingService {
    /// Create a new OpenAI embedding service.
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            api_key,
            model,
        }
    }

    /// Create with default OpenAI configuration.
    pub fn default_config(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key,
            model: "text-embedding-3-small".to_string(),
        }
    }
}

#[async_trait]
impl EmbeddingService for OpenAIEmbeddingService {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let request = OpenAIEmbedRequest {
            prompt: text.to_string(),
            input: text.to_string(),
            model: self.model.clone(),
            dimensions: self.dimension(),
            encoding_format: "float".to_string(),
        };

        let url = format!("{}/embeddings", self.base_url);
        tracing::debug!(
            "Sending embedding request to OpenAI: url={}, model={}, text_len={}",
            url,
            self.model,
            text.len()
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Failed to send embedding request to OpenAI at {}",
                    self.base_url
                )
            })?;

        let status = response.status();
        tracing::debug!("OpenAI response status: {}", status);

        let result: OpenAIEmbedResponse = response
            .error_for_status()
            .with_context(|| format!("OpenAI returned error status: {}", status))?
            .json()
            .await
            .context("Failed to parse OpenAI embedding response")?;

        tracing::debug!(
            "OpenAI embedding received: dimension={}",
            result.data[0].embedding.len()
        );

        Ok(result.data[0].embedding.clone())
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let request = OpenAIEmbedBatchRequest {
            input: texts.to_vec(),
            model: self.model.clone(),
            dimensions: self.dimension(),
            encoding_format: "float".to_string(),
        };

        let response = self
            .client
            .post(format!("{}/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send batch embedding request to OpenAI")?;

        let result: OpenAIEmbedResponse = response
            .error_for_status()?
            .json()
            .await
            .context("Failed to parse OpenAI batch embedding response")?;

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        // text-embedding-3-small default dimension
        1536
    }
}

#[derive(serde::Serialize)]
struct OpenAIEmbedRequest {
    model: String,
    input: String,
    prompt: String,
    dimensions: usize,
    encoding_format: String,
}

#[derive(serde::Serialize)]
struct OpenAIEmbedBatchRequest {
    input: Vec<String>,
    model: String,
    dimensions: usize,
    encoding_format: String,
}

#[derive(serde::Deserialize)]
struct OpenAIEmbedResponse {
    data: Vec<OpenAIEmbedData>,
}

#[derive(serde::Deserialize)]
struct OpenAIEmbedData {
    embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_default_config() {
        let service = OllamaEmbeddingService::default_config();
        assert_eq!(service.base_url, "http://localhost:11434");
        assert_eq!(service.model, "nomic-embed-text");
        assert_eq!(service.dimension(), 768);
    }

    #[test]
    fn test_openai_dimension() {
        let service = OpenAIEmbeddingService::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "text-embedding-3-small".to_string(),
        );
        assert_eq!(service.dimension(), 1536);
    }
}
