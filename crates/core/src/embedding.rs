#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_is_reordered_by_index() {
        let body = serde_json::json!({
            "model": "voyage-3-large",
            "data": [
                {"index": 1, "embedding": [3.0, 4.0]},
                {"index": 0, "embedding": [1.0, 2.0]}
            ]
        });
        assert_eq!(
            parse_response(body, "voyage-3-large", 2, 2).unwrap(),
            vec![vec![1.0, 2.0], vec![3.0, 4.0]]
        );
    }

    #[test]
    fn response_rejects_missing_duplicate_and_out_of_range_indices() {
        for body in [
            serde_json::json!({"model":"m","data":[{"index":0,"embedding":[1.0] }]}),
            serde_json::json!({"model":"m","data":[{"index":0,"embedding":[1.0]},{"index":0,"embedding":[2.0]}]}),
            serde_json::json!({"model":"m","data":[{"index":0,"embedding":[1.0]},{"index":2,"embedding":[2.0]}]}),
        ] {
            assert!(parse_response(body, "m", 1, 2).is_err());
        }
    }

    #[test]
    fn response_rejects_model_or_dimension_mismatch() {
        let wrong_model =
            serde_json::json!({"model":"other","data":[{"index":0,"embedding":[1.0,2.0]}]});
        assert!(parse_response(wrong_model, "m", 2, 1).is_err());
        let wrong_dimension =
            serde_json::json!({"model":"m","data":[{"index":0,"embedding":[1.0]}]});
        assert!(parse_response(wrong_dimension, "m", 2, 1).is_err());
    }
}
use std::time::Duration;

use anyhow::{Context, ensure};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::Config;

#[derive(Debug, Clone, Copy)]
pub enum InputKind {
    Document,
    Query,
}

#[derive(Debug, Clone)]
pub struct EmbeddingClient {
    http: Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    dimensions: usize,
    document_input_type: String,
    query_input_type: String,
}

#[derive(Serialize)]
struct Request<'a> {
    input: &'a [String],
    model: &'a str,
    input_type: &'a str,
    output_dimension: usize,
}

#[derive(Deserialize)]
struct Response {
    model: String,
    data: Vec<ResponseItem>,
}

#[derive(Deserialize)]
struct ResponseItem {
    index: usize,
    embedding: Vec<f32>,
}

impl EmbeddingClient {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_millis(config.embedding_timeout_ms))
            .build()?;
        Ok(Self {
            http,
            base_url: config.embedding_base_url.trim_end_matches('/').to_string(),
            api_key: config
                .embedding_api_key
                .clone()
                .filter(|key| !key.trim().is_empty()),
            model: config.embedding_model.clone(),
            dimensions: config.embedding_dimensions,
            document_input_type: config.embedding_input_type_document.clone(),
            query_input_type: config.embedding_input_type_query.clone(),
        })
    }

    pub fn available(&self) -> bool {
        self.api_key.is_some()
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
    pub fn document_input_type(&self) -> &str {
        &self.document_input_type
    }

    pub async fn embed(&self, input: &[String], kind: InputKind) -> anyhow::Result<Vec<Vec<f32>>> {
        ensure!(!input.is_empty(), "input embedding tidak boleh kosong");
        let key = self
            .api_key
            .as_deref()
            .context("EMBEDDING_API_KEY belum diisi")?;
        let input_type = match kind {
            InputKind::Document => &self.document_input_type,
            InputKind::Query => &self.query_input_type,
        };
        let response = self
            .http
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(key)
            .json(&Request {
                input,
                model: &self.model,
                input_type,
                output_dimension: self.dimensions,
            })
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        parse_response(response, &self.model, self.dimensions, input.len())
    }
}

fn parse_response(
    value: serde_json::Value,
    expected_model: &str,
    dimensions: usize,
    count: usize,
) -> anyhow::Result<Vec<Vec<f32>>> {
    let response: Response =
        serde_json::from_value(value).context("respons embedding tidak sah")?;
    ensure!(
        response.model == expected_model,
        "model respons embedding tidak cocok"
    );
    ensure!(response.data.len() == count, "jumlah embedding tidak cocok");
    let mut ordered = vec![None; count];
    for item in response.data {
        ensure!(item.index < count, "index embedding di luar rentang");
        ensure!(
            item.embedding.len() == dimensions,
            "dimensi embedding tidak cocok"
        );
        ensure!(
            ordered[item.index].replace(item.embedding).is_none(),
            "index embedding duplikat"
        );
    }
    ordered
        .into_iter()
        .enumerate()
        .map(|(index, item)| item.with_context(|| format!("embedding index {index} hilang")))
        .collect()
}
