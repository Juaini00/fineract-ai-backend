use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{
    compose,
    proposal::{Proposal, Validation},
    repository::SettledResponse,
};
use crate::catalog::Catalog;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    Analysis,
    Greeting,
    Thanks,
    Acknowledgement,
    Identity,
    Help,
    ClarifyGoal,
    ExplainResult,
    ScopeHelp,
    EnglishRequired,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Understanding {
    schema_version: u32,
    intent: Intent,
    analysis: Option<Value>,
}

pub static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    let mut analysis = super::proposal::RESPONSE_SCHEMA.clone();
    let definitions = analysis
        .as_object_mut()
        .expect("proposal schema")
        .remove("$defs");
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["schema_version", "intent", "analysis"],
        "properties": {
            "schema_version": {"type":"integer", "const":1},
            "intent": {"enum":["analysis","greeting","thanks","acknowledgement","identity","help","clarify_goal","explain_result","scope_help","english_required"]},
            "analysis": {"anyOf":[{"type":"null"},analysis]}
        },
        "$defs": definitions
    })
});
pub static SCHEMA_BYTES: LazyLock<usize> = LazyLock::new(|| SCHEMA.to_string().len());

pub fn parse(text: &str) -> Result<Result<Proposal, Intent>, Validation> {
    let unique: super::proposal::UniqueValue =
        serde_json::from_str(text).map_err(|_| Validation::new("invalid_understanding_json"))?;
    if unique.0.get("analysis").is_none() {
        return Err(Validation::new("missing_analysis_field"));
    }
    let value: Understanding = serde_json::from_value(unique.0)
        .map_err(|_| Validation::new("invalid_understanding_schema"))?;
    if value.schema_version != 1 {
        return Err(Validation::new("invalid_understanding_version"));
    }
    match (value.intent, value.analysis) {
        (Intent::Analysis, Some(proposal)) => Proposal::parse(&proposal.to_string()).map(Ok),
        (Intent::Analysis, None) | (_, Some(_)) => Err(Validation::new("intent_payload_mismatch")),
        (intent, None) => Ok(Err(intent)),
    }
}

pub fn response(intent: Intent, catalog: &Catalog) -> SettledResponse {
    let mut available: Vec<_> = if intent == Intent::Help {
        catalog
            .capabilities
            .iter()
            .filter(|c| c.entry.status.as_deref() == Some("approved_mvp"))
            .filter(|c| {
                c.entry
                    .query_id
                    .as_ref()
                    .is_some_and(|id| catalog.queries.iter().any(|q| &q.entry.id == id))
            })
            .map(|c| {
                (
                    c.entry.id.as_str(),
                    c.entry.display_name.as_deref().unwrap_or(&c.entry.id),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    available.sort_unstable_by_key(|entry| (entry.1, entry.0));
    available.truncate(6);
    let body = match intent {
        Intent::Greeting => "Hello! I'm Jarvis, your read-only Fineract data-analysis assistant. What would you like to explore?".to_string(),
        Intent::Thanks => "You're welcome. Let me know if you need help with another Fineract analysis.".to_string(),
        Intent::Acknowledgement => "Understood. Let me know what you'd like to explore next.".to_string(),
        Intent::Identity => "I'm Jarvis, a read-only Fineract data-analysis assistant. I help you explore approved data; I cannot create, change or delete banking records.".to_string(),
        Intent::Help => {
            format!("I can help you analyze approved Fineract data within your authorized office scope. Available analyses include: {}. Tell me the subject, reporting period and any currency or office constraints. I am read-only and cannot book services or change banking records. Multi-stage clarification and automatic reuse of previous results are not currently available; include the details needed for each analysis.", available.iter().map(|entry|entry.1).collect::<Vec<_>>().join("; "))
        }
        Intent::ExplainResult => "Please identify the result you want explained and include its relevant details. Automatic reuse of previous results is not currently available. I will not infer figures or context from an unspecified earlier result.".to_string(),
        Intent::ScopeHelp => "I can help with approved, read-only Fineract data analysis. I cannot perform unrelated services or take actions outside that scope. Tell me which Fineract data you would like to explore.".to_string(),
        Intent::EnglishRequired => "Please restate your request in English. English is the supported language for Jarvis.".to_string(),
        Intent::ClarifyGoal => "What would you like to report on? Please specify the Fineract subject, reporting period and any currency or office constraints, so I do not choose an analysis for you.".to_string(),
        Intent::Analysis => unreachable!("analytical turns continue through the Engine"),
    };
    let blocks = json!([compose::block(
        "assistant_reply",
        "narrative",
        &[],
        json!({"body":body})
    )]);
    SettledResponse {
        kind: if intent == Intent::ScopeHelp {
            "limitation"
        } else {
            "conversation"
        },
        outcome: if intent == Intent::ScopeHelp {
            "Unsupported"
        } else {
            "Answered"
        },
        completeness: if intent == Intent::ScopeHelp {
            "Unknown"
        } else {
            "Complete"
        },
        completeness_reason: if intent == Intent::ScopeHelp {
            "outside_product_scope"
        } else {
            "conversation_only_no_analytical_query"
        }
        .into(),
        response_hash: hex::encode(Sha256::digest(blocks.to_string().as_bytes())),
        blocks,
        evidence: json!({
            "purpose":"conversation", "analytical_queries":0,
            "intent": intent,
            "catalog_content_hash": catalog.content_hash,
            "help_capability_ids": if intent == Intent::Help { available.iter().map(|entry| entry.0).collect::<Vec<_>>() } else { vec![] },
            "context_reuse": "not_available"
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn social_turn_cannot_smuggle_an_analytical_plan() {
        assert!(parse(r#"{"schema_version":1,"intent":"greeting","analysis":{"schema_version":1,"decision":"planned","confidence":"high","nodes":[{"id":"n","capability_id":"x","depends_on":[],"slots":{}}]}}"#).is_err());
        assert!(matches!(
            parse(r#"{"schema_version":1,"intent":"greeting","analysis":null}"#),
            Ok(Err(Intent::Greeting))
        ));
    }
    #[test]
    fn only_scope_help_becomes_an_unsupported_limitation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = crate::catalog::loader::load(&root.join("knowledge"), &root.join("queries"))
            .expect("catalog loads");
        for intent in [
            Intent::Greeting,
            Intent::Thanks,
            Intent::Acknowledgement,
            Intent::Identity,
            Intent::Help,
            Intent::ClarifyGoal,
            Intent::ExplainResult,
            Intent::EnglishRequired,
        ] {
            let r = response(intent, &catalog);
            assert_eq!(r.kind, "conversation", "{intent:?}");
            assert_eq!(r.outcome, "Answered", "{intent:?}");
            assert_eq!(r.completeness, "Complete", "{intent:?}");
            assert_eq!(r.evidence["analytical_queries"], json!(0), "{intent:?}");
        }
        let r = response(Intent::ScopeHelp, &catalog);
        assert_eq!((r.kind, r.outcome, r.completeness), ("limitation", "Unsupported", "Unknown"));
    }

    #[test]
    fn rejects_duplicate_intent_unknown_fields_and_missing_analysis() {
        for text in [
            r#"{"schema_version":1,"intent":"analysis","intent":"greeting","analysis":null}"#,
            r#"{"schema_version":1,"intent":"greeting","analysis":null,"sql":"DELETE"}"#,
            r#"{"schema_version":1,"intent":"analysis","analysis":null}"#,
        ] {
            assert!(parse(text).is_err());
        }
    }
}
