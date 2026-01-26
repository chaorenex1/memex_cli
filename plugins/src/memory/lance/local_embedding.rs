//! Local CPU/GPU embedding service using Candle.
//!
//! This module provides embedding functionality using local models
//! running on CPU, CUDA (NVIDIA GPU), or Metal (Apple Silicon).

use anyhow::Result;
use async_trait::async_trait;

use super::embedding::EmbeddingService;

/// Configuration for local embedding.
#[derive(Debug, Clone)]
pub struct LocalEmbeddingConfig {
    pub model: String,
    pub repo: String,
    pub device: String,
    pub dimension: usize,
}

impl LocalEmbeddingConfig {
    pub fn new(model: String, device: String, dimension: usize) -> Self {
        Self {
            model,
            repo: "sentence-transformers".to_string(),
            device,
            dimension,
        }
    }

    pub fn with_repo(mut self, repo: String) -> Self {
        self.repo = repo;
        self
    }
}

/// Local CPU/GPU embedding service.
///
/// This service loads a transformer model locally and performs
/// embedding inference on CPU, CUDA, or Metal devices.
///
/// # Note
///
/// The actual implementation is gated behind the `local-embedding` feature.
/// When the feature is not enabled, this service will return an error.
pub struct LocalEmbeddingService {
    #[allow(dead_code)]
    config: LocalEmbeddingConfig,
}

impl LocalEmbeddingService {
    /// Create a new local embedding service.
    pub fn new(config: LocalEmbeddingConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration (CPU, all-MiniLM-L6-v2).
    pub fn default_config() -> Self {
        Self {
            config: LocalEmbeddingConfig {
                model: "all-MiniLM-L6-v2".to_string(),
                repo: "sentence-transformers".to_string(),
                device: "cpu".to_string(),
                dimension: 384,
            },
        }
    }
}

#[async_trait]
impl EmbeddingService for LocalEmbeddingService {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        #[cfg(feature = "local-embedding")]
        {
            self.embed_impl(text).await
        }

        #[cfg(not(feature = "local-embedding"))]
        {
            let _text = text; // Suppress unused warning when feature is disabled
            Err(anyhow::anyhow!(
                "Local embedding feature is not enabled. \
                 Please rebuild with: cargo build --features local-embedding"
            ))
        }
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        #[cfg(feature = "local-embedding")]
        {
            self.embed_batch_impl(texts).await
        }

        #[cfg(not(feature = "local-embedding"))]
        {
            let _texts = texts; // Suppress unused warning when feature is disabled
            Err(anyhow::anyhow!(
                "Local embedding feature is not enabled. \
                 Please rebuild with: cargo build --features local-embedding"
            ))
        }
    }

    fn dimension(&self) -> usize {
        self.config.dimension
    }
}

#[cfg(feature = "local-embedding")]
mod local_impl {
    use super::*;
    use candle_core::{Device, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::bert::{BertModel, Config as BertConfig};
    use hf_hub::{api::sync::ApiBuilder, Cache};
    use tokenizers::Tokenizer;

    impl LocalEmbeddingService {
        async fn embed_impl(&self, text: &str) -> Result<Vec<f32>> {
            let tokenizer = self.get_tokenizer()?;
            let model = self.get_model()?;

            // Tokenize input
            let tokens = tokenizer
                .encode(text, true)
                .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?
                .get_ids()
                .to_vec();

            // Create input tensor
            let input = Tensor::new(&tokens[..], &Device::Cpu)?.unsqueeze(0)?;

            // Run model
            let embeddings = model.forward(&input, None, None)?;

            // Mean pooling
            let result = self.mean_pooling(embeddings)?;

            // Convert to Vec<f32>
            let result_data = result.to_vec1::<f32>()?;
            Ok(result_data)
        }

        async fn embed_batch_impl(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            let mut results = Vec::with_capacity(texts.len());
            for text in texts {
                results.push(self.embed(text).await?);
            }
            Ok(results)
        }

        fn get_tokenizer(&self) -> Result<Tokenizer> {
            let api = ApiBuilder::new().with_token(None).build()?;
            let repo = api.model(format!("{}/{}", self.config.repo, self.config.model));

            let tokenizer_path = repo.get("tokenizer.json")?;
            Tokenizer::from_file(tokenizer_path)
                .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))
        }

        fn get_model(&self) -> Result<BertModel> {
            let device = self.parse_device()?;

            let api = ApiBuilder::new().with_token(None).build()?;
            let repo = api.model(format!("{}/{}", self.config.repo, self.config.model));

            let config_path = repo.get("config.json")?;
            let weights_path = repo.get("model.safetensors")?;

            // Load config
            let config: BertConfig = serde_json::from_slice(&std::fs::read(config_path)?)?;

            // Load weights
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(
                    &[weights_path],
                    candle_core::DType::F32,
                    &device,
                )?
            };

            Ok(BertModel::new(vb, &config)?)
        }

        fn parse_device(&self) -> Result<Device> {
            match self.config.device.to_lowercase().as_str() {
                "cpu" => Ok(Device::Cpu),
                "cuda" => Ok(Device::new_cuda(0)?),
                "metal" => Ok(Device::new_metal(0)?),
                d => Err(anyhow::anyhow!("Unsupported device: {}", d)),
            }
        }

        fn mean_pooling(&self, embeddings: Tensor) -> Result<Tensor> {
            // Simple mean pooling over the sequence length
            let (_batch_size, _seq_len, hidden_size) = embeddings.dims3()?;
            let mean = embeddings.mean(1)?;
            Ok(mean)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_config() {
        let config =
            LocalEmbeddingConfig::new("all-MiniLM-L6-v2".to_string(), "cpu".to_string(), 384);
        assert_eq!(config.model, "all-MiniLM-L6-v2");
        assert_eq!(config.device, "cpu");
        assert_eq!(config.dimension, 384);
    }

    #[test]
    fn test_local_default_config() {
        let service = LocalEmbeddingService::default_config();
        assert_eq!(service.config.model, "all-MiniLM-L6-v2");
        assert_eq!(service.config.device, "cpu");
        assert_eq!(service.dimension(), 384);
    }
}
