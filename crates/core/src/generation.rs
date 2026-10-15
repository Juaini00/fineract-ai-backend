use std::{sync::Mutex, time::Duration};

use rig_core::{
    client::CompletionClient,
    completion::{AssistantContent, CompletionModel},
    providers::openai,
};
use serde_json::{Value, json};
use tracing::Instrument;

use crate::{
    Config,
    commit_isolation::{self, ExternalCall},
};

pub struct GenerationClient {
    model: Option<openai::completion::CompletionModel>,
    timeout: Duration,
    max_output_tokens: u64,
    temperature: f64,
    observation: Mutex<Observation>,
}

#[derive(Debug, Default)]
struct Observation {
    checked: bool,
    available: bool,
    proposal_verified: bool,
}

impl std::fmt::Debug for GenerationClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenerationClient")
            .field("configured", &self.model.is_some())
            .finish_non_exhaustive()
    }
}

pub struct Generated {
    pub text: String,
    pub model: String,
    pub usage: Option<Value>,
    pub total_tokens: Option<i64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub complete: bool,
}

pub struct GenerationFailure {
    pub retryable: bool,
}

impl GenerationClient {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let configured = !config.llm_provider.is_empty()
            && !config.llm_model.is_empty()
            && !config.llm_chat_completions_url.is_empty();
        let timeout = Duration::from_millis(config.llm_timeout_ms);
        let model = if configured {
            let base = config
                .llm_chat_completions_url
                .strip_suffix("/chat/completions")
                .ok_or_else(|| anyhow::anyhow!("invalid LLM_CHAT_COMPLETIONS_URL"))?;
            let http = reqwest::Client::builder()
                .timeout(timeout)
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            let client = openai::CompletionsClient::builder()
                .api_key(config.llm_api_key.as_deref().unwrap_or(""))
                .base_url(base)
                .http_client(http)
                .build()
                .map_err(|_| anyhow::anyhow!("could not construct configured Rig client"))?;
            Some(client.completion_model(&config.llm_model))
        } else {
            None
        };
        Ok(Self {
            model,
            timeout,
            max_output_tokens: config.llm_max_output_tokens,
            temperature: config.llm_temperature,
            observation: Mutex::new(Observation::default()),
        })
    }

    pub fn configured(&self) -> bool {
        self.model.is_some()
    }

    pub fn settings(&self) -> Value {
        let state = self
            .observation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        json!({ "implementation": "integrated", "configured": self.configured(),
            "connection_checked": state.checked, "operational": state.checked && state.available,
            "proposal_verified": state.proposal_verified })
    }

    pub fn record_verified(&self) {
        self.observation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .proposal_verified = true;
    }

    pub async fn generate(
        &self,
        system: &str,
        input: &str,
        request_text: &str,
        schema: &Value,
        remaining: Duration,
    ) -> Result<Generated, GenerationFailure> {
        let Some(model) = &self.model else {
            return Err(GenerationFailure { retryable: false });
        };
        // llama.cpp's active Qwen template reports supports_typed_content=false.
        // Rig 0.40 emits typed system content; override the text-only wire
        // messages rather than dropping instructions or adding another client.
        let request = model
            .completion_request("")
            .temperature(self.temperature)
            .max_tokens(self.max_output_tokens)
            .additional_params(json!({
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": input },
                    { "role": "user", "content": request_text },
                ],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "planner_v1",
                        "strict": true,
                        "schema": schema,
                    }
                },
            }))
            .build();
        commit_isolation::guard(ExternalCall::Generation);
        // Rig's TRACE target contains wire bodies. Disable it globally and
        // supply an application span with no prompt or completion fields.
        let result = tokio::time::timeout(
            self.timeout.min(remaining),
            model
                .completion(request)
                .instrument(tracing::info_span!("planner_generation")),
        )
        .await;
        let mut state = self
            .observation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.checked = true;
        match result {
            Ok(Ok(response)) => {
                state.available = true;
                let usage = response.raw_response.usage.as_ref().map(|_| {
                    json!({
                        "input_tokens": response.usage.input_tokens,
                        "output_tokens": response.usage.output_tokens,
                        "total_tokens": response.usage.total_tokens,
                        "cached_input_tokens": response.usage.cached_input_tokens,
                    })
                });
                let observed = response.raw_response.usage.is_some()
                    && (response.usage.total_tokens > 0
                        || response.usage.input_tokens > 0
                        || response.usage.output_tokens > 0);
                let complete = response.raw_response.choices.len() == 1
                    && response.raw_response.choices[0].finish_reason == "stop";
                let (text, only_text) = answer_text(response.choice);
                Ok(Generated {
                    text,
                    model: response.raw_response.model,
                    usage,
                    total_tokens: observed
                        .then(|| {
                            i64::try_from(
                                response.usage.total_tokens.max(
                                    response
                                        .usage
                                        .input_tokens
                                        .saturating_add(response.usage.output_tokens),
                                ),
                            )
                            .ok()
                        })
                        .flatten(),
                    input_tokens: observed.then_some(response.usage.input_tokens),
                    output_tokens: observed.then_some(response.usage.output_tokens),
                    complete: complete && only_text,
                })
            }
            Ok(Err(error)) => {
                state.available = false;
                let retryable = error.provider_response_status().is_none_or(|status| {
                    status == 408 || status == 429 || status.is_server_error()
                });
                Err(GenerationFailure { retryable })
            }
            Err(_) => {
                state.available = false;
                Err(GenerationFailure { retryable: true })
            }
        }
    }
}

/// Reasoning-capable providers return hidden reasoning in a separate
/// `reasoning_content` field, which Rig exposes as a Reasoning part. It is never
/// the answer: drop it, then require exactly one Text part.
fn answer_text(choice: impl IntoIterator<Item = AssistantContent>) -> (String, bool) {
    let mut parts = choice
        .into_iter()
        .filter(|part| !matches!(part, AssistantContent::Reasoning(_)));
    match (parts.next(), parts.next()) {
        (Some(AssistantContent::Text(content)), None) => (content.text, true),
        _ => (String::new(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig_core::completion::message::Reasoning;

    #[test]
    fn reasoning_part_beside_one_text_is_complete() {
        let parts = [
            AssistantContent::Reasoning(Reasoning::new("thinking")),
            AssistantContent::text("{}"),
        ];
        assert_eq!(answer_text(parts), ("{}".to_string(), true));
    }

    #[test]
    fn reasoning_only_or_two_texts_stay_incomplete() {
        assert!(!answer_text([AssistantContent::Reasoning(Reasoning::new("x"))]).1);
        assert!(!answer_text([AssistantContent::text("a"), AssistantContent::text("b")]).1);
    }
}
