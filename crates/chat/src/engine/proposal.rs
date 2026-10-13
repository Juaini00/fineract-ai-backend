use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::LazyLock,
};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};

pub(crate) static RESPONSE_SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "decision", "confidence", "nodes"],
        "properties": {
            "schema_version": { "type": "integer", "const": 1 },
            "decision": { "enum": ["planned", "abstain", "no_fit"] },
            "confidence": { "enum": ["low", "medium", "high"] },
            "nodes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "capability_id", "depends_on", "slots"],
                    "properties": {
                        "id": { "type": "string", "minLength": 1 },
                        "capability_id": { "type": "string", "minLength": 1 },
                        "depends_on": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 }
                        },
                        "slots": {
                            "type": "object",
                            "additionalProperties": {
                                "oneOf": [
                                    {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "required": ["kind", "value", "source"],
                                        "properties": {
                                            "kind": { "const": "value" },
                                            "value": {},
                                            "source": { "$ref": "#/$defs/source" }
                                        }
                                    },
                                    {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "required": ["kind", "source"],
                                        "properties": {
                                            "kind": { "const": "date_phrase" },
                                            "source": { "$ref": "#/$defs/source" }
                                        }
                                    },
                                    {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "required": ["kind", "arguments"],
                                        "properties": {
                                            "kind": { "const": "probe" },
                                            "arguments": {
                                                "type": "object",
                                                "additionalProperties": { "$ref": "#/$defs/argument" }
                                            }
                                        }
                                    },
                                    {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "required": ["kind", "node_id", "output_slot"],
                                        "properties": {
                                            "kind": { "const": "node_output" },
                                            "node_id": { "type": "string", "minLength": 1 },
                                            "output_slot": { "type": "string", "minLength": 1 }
                                        }
                                    }
                                ]
                            }
                        }
                    }
                }
            }
        },
        "$defs": {
            "source": {
                "type": "object",
                "additionalProperties": false,
                "required": ["quote", "occurrence"],
                "properties": {
                    "quote": { "type": "string", "minLength": 1 },
                    "occurrence": { "type": "integer", "minimum": 0 }
                }
            },
            "argument": {
                "type": "object",
                "additionalProperties": false,
                "required": ["value", "source"],
                "properties": {
                    "value": {},
                    "source": { "$ref": "#/$defs/source" }
                }
            }
        }
    })
});

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub schema_version: u32,
    pub decision: Decision,
    pub confidence: Confidence,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Planned,
    Abstain,
    NoFit,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub capability_id: String,
    pub depends_on: Vec<String>,
    pub slots: BTreeMap<String, Slot>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Slot {
    Value {
        value: Value,
        source: Source,
    },
    DatePhrase {
        source: Source,
    },
    Probe {
        arguments: BTreeMap<String, Argument>,
    },
    NodeOutput {
        node_id: String,
        output_slot: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Argument {
    pub value: Value,
    pub source: Source,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub quote: String,
    pub occurrence: usize,
}

impl Source {
    pub fn span(&self, request: &str) -> Result<(usize, usize), &'static str> {
        if self.quote.is_empty() {
            return Err("empty_source_quote");
        }
        let mut matches = request.match_indices(&self.quote);
        let first = matches.next().ok_or("source_quote_not_found")?;
        let (start, matched) = if self.occurrence == 0 {
            first
        } else {
            matches.nth(self.occurrence - 1).ok_or("source_occurrence_not_found")?
        };
        Ok((start, start + matched.len()))
    }
}

#[derive(Debug, Serialize)]
pub struct Validation {
    pub code: &'static str,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub category: Option<&'static str>,
}

impl Validation {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            line: None,
            column: None,
            category: None,
        }
    }
}

impl Proposal {
    pub fn parse(text: &str) -> Result<Self, Validation> {
        // Value's normal deserializer keeps the last duplicate map key. Reject
        // duplicates recursively before deserializing the closed tagged schema.
        let unique: UniqueValue = serde_json::from_str(text).map_err(|error| Validation {
            code: "invalid_json",
            line: Some(error.line()),
            column: Some(error.column()),
            category: Some(match error.classify() {
                serde_json::error::Category::Syntax => "syntax",
                serde_json::error::Category::Data => "duplicate_key_or_invalid_type",
                serde_json::error::Category::Eof => "unexpected_end",
                serde_json::error::Category::Io => "io",
            }),
        })?;
        let proposal: Self =
            serde_json::from_value(unique.0).map_err(|_| Validation::new("invalid_schema"))?;
        if proposal.schema_version != 1 {
            return Err(Validation::new("invalid_schema_version"));
        }
        if (proposal.decision == Decision::Planned) == proposal.nodes.is_empty() {
            return Err(Validation::new("decision_nodes_mismatch"));
        }
        let ids: BTreeSet<_> = proposal.nodes.iter().map(|node| node.id.as_str()).collect();
        if ids.len() != proposal.nodes.len() || ids.contains("") {
            return Err(Validation::new("invalid_node_ids"));
        }
        let mut visited = BTreeSet::new();
        for node in &proposal.nodes {
            let dependencies: BTreeSet<_> = node.depends_on.iter().collect();
            if dependencies.len() != node.depends_on.len()
                || dependencies
                    .iter()
                    .any(|id| !ids.contains(id.as_str()) || **id == node.id)
            {
                return Err(Validation::new("invalid_dependencies"));
            }
            for slot in node.slots.values() {
                if let Slot::NodeOutput {
                    node_id,
                    output_slot,
                } = slot
                    && (!dependencies.contains(node_id) || output_slot.is_empty())
                {
                    return Err(Validation::new("invalid_output_binding"));
                }
            }
        }
        while visited.len() < ids.len() {
            let before = visited.len();
            for node in &proposal.nodes {
                if node
                    .depends_on
                    .iter()
                    .all(|id| visited.contains(id.as_str()))
                {
                    visited.insert(node.id.as_str());
                }
            }
            if before == visited.len() {
                return Err(Validation::new("dependency_cycle"));
            }
        }
        Ok(proposal)
    }
}

struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut result = serde_json::Map::new();
                while let Some((key, value)) = map.next_entry::<String, UniqueValue>()? {
                    if result.insert(key, value.0).is_some() {
                        return Err(de::Error::custom("duplicate key"));
                    }
                }
                Ok(UniqueValue(Value::Object(result)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut result = Vec::new();
                while let Some(value) = seq.next_element::<UniqueValue>()? {
                    result.push(value.0);
                }
                Ok(UniqueValue(Value::Array(result)))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_json_and_closed_schema_violations() {
        let valid = r#"{"schema_version":1,"decision":"abstain","confidence":"low","nodes":[]}"#;
        assert!(Proposal::parse(valid).is_ok());
        for invalid in [
            format!("```json\n{valid}\n```"),
            format!("{valid} trailing"),
            valid.replace("\"schema_version\":1", "\"schema_version\":\"1\""),
            valid.replace("\"nodes\":[]", "\"nodes\":[],\"sql\":\"SELECT 1\""),
            valid.replace("\"nodes\":[]", "\"nodes\":[],\"nodes\":[]"),
            valid.replace("abstain", "planned"),
        ] {
            assert!(Proposal::parse(&invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn rejects_duplicate_slot_keys_and_cycles() {
        let prefix = r#"{"schema_version":1,"decision":"planned","confidence":"high","nodes": ["#;
        let duplicate = r#"{"id":"n1","capability_id":"c","depends_on":[],"slots":{"x":{"kind":"probe","arguments":{}},"x":{"kind":"probe","arguments":{}}}}"#;
        assert!(Proposal::parse(&format!("{prefix}{duplicate}]}}")).is_err());
        let cycle = r#"{"id":"a","capability_id":"c","depends_on":["b"],"slots":{}},{"id":"b","capability_id":"c","depends_on":["a"],"slots":{}}"#;
        assert_eq!(
            Proposal::parse(&format!("{prefix}{cycle}]}}"))
                .unwrap_err()
                .code,
            "dependency_cycle"
        );
    }
    #[test]
    fn grounds_utf8_spans_and_repeated_occurrences() {
        let request = "é bulan ini, bulan ini";
        assert_eq!(
            Source {
                quote: "bulan ini".into(),
                occurrence: 1
            }
            .span(request),
            Ok((14, 23))
        );
        assert!(
            Source {
                quote: "bulan ini".into(),
                occurrence: 2
            }
            .span(request)
            .is_err()
        );
        assert!(
            Source {
                quote: String::new(),
                occurrence: 0
            }
            .span(request)
            .is_err()
        );
    }
}
