//! Fakta session yang dipromosikan pada commit response (T7).
//!
//! Satu-satunya titik promosi adalah commit response (memory-context.md §3):
//! fakta memori mustahil ada tanpa response durable, dan K4 menegakkannya lewat
//! FK komposit ke `(source_job_id, source_response_version)`. Karena itu modul
//! ini hanya **menurunkan** fakta dari rencana dan response yang sudah selesai
//! disusun — ia tidak menyimpulkan apa pun sendiri, dan tidak ada jalur tulis
//! inkremental dari node, resolver, atau checkpoint.
//!
//! Angka di dalam fakta bukan sumber otoritatif. Ia referensi untuk follow-up;
//! angka tetap berasal dari response dan ledger (I7).

use serde_json::{Value, json};

use crate::{
    clarification::repository::AnsweredIdentity,
    engine::{
        planner::{Bound, Plan},
        repository::SettledResponse,
    },
};

/// Satu baris `session_memory` yang akan ditulis dalam transaksi commit.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryFact {
    pub kind: &'static str,
    pub entity_key: Option<String>,
    /// Berpotensi PII (label nasabah). Ikut terhapus lewat CASCADE session.
    pub label: Option<String>,
    pub fact_json: Value,
    pub completeness: String,
    pub completeness_reason: String,
    pub provenance_json: Value,
}

/// Fakta yang dipromosikan oleh satu response `analysis`.
///
/// Urutannya menentukan `session_seq`: identitas dan scope lebih dulu, hasilnya
/// terakhir — sehingga fakta dengan seq tertinggi adalah hasil yang baru saja
/// dijawab.
///
/// Response `limitation` tidak memanggil fungsi ini: tidak ada hasil, tidak ada
/// identitas yang terbukti dipakai, dan tidak ada scope yang menghasilkan
/// apa pun. Mempromosikan fakta dari jawaban yang tidak menjawab berarti
/// pertanyaan berikutnya membawa konteks yang tidak pernah terbukti.
pub fn promoted(
    plan: &Plan,
    response: &SettledResponse,
    supplied: &[AnsweredIdentity],
    row_count: usize,
) -> Vec<MemoryFact> {
    let mut facts: Vec<MemoryFact> = supplied
        .iter()
        .map(|answer| resolved_entity(answer, response))
        .collect();

    facts.push(active_scope(plan, response));
    facts.push(prior_result(plan, response, row_count));
    facts
}

/// Identitas yang terbukti dipakai response ini.
///
/// `provenance` dibawa apa adanya: `resolver_unique` (K5 — tidak ada yang
/// mengonfirmasinya) tidak pernah diratakan menjadi `user_confirmed`. Jawaban
/// teks bebas tanpa binding tidak sampai ke sini: repository hanya mengembalikan
/// jawaban yang punya `binding_json.value` (C10).
fn resolved_entity(answer: &AnsweredIdentity, response: &SettledResponse) -> MemoryFact {
    MemoryFact {
        kind: "ResolvedEntity",
        entity_key: Some(answer.field_id.clone()),
        label: answer.label.clone(),
        fact_json: json!({
            "field_id": answer.field_id,
            "value": answer.value,
            "provenance": answer.provenance,
            "resolver_ref": answer.resolver_ref,
        }),
        completeness: response.completeness.to_string(),
        completeness_reason: response.completeness_reason.clone(),
        provenance_json: json!({ "source": "clarification_answer" }),
    }
}

/// Scope yang benar-benar dipakai response: office terotorisasi dan periode
/// yang terikat. Daftar office disimpan utuh di sini — berbeda dari response,
/// yang hanya melaporkan jumlahnya — karena follow-up perlu scope yang sama
/// persis, bukan hitungannya.
fn active_scope(plan: &Plan, response: &SettledResponse) -> MemoryFact {
    let mut period = serde_json::Map::new();
    let mut office_ids: Vec<i64> = Vec::new();

    for (name, bound) in plan.parameter_names.iter().zip(&plan.parameters) {
        match bound {
            Bound::OfficeIds(ids) => office_ids = ids.clone(),
            Bound::Date(date) => {
                period.insert(name.clone(), Value::String(date.to_string()));
            }
            _ => {}
        }
    }

    MemoryFact {
        kind: "ActiveScope",
        // Constraint `session_memory_scope_shape`: scope tidak punya entity_key
        // maupun dataset_id.
        entity_key: None,
        label: None,
        fact_json: json!({ "office_ids": office_ids, "period": period }),
        completeness: response.completeness.to_string(),
        completeness_reason: response.completeness_reason.clone(),
        provenance_json: json!({ "source": "authorized_scope" }),
    }
}

/// Referensi ke hasil yang baru saja di-commit.
///
/// `provenance_json` diambil dari blok `provenance` response, bukan disusun
/// ulang: dua penyusunan yang sama-sama benar hari ini akan menyimpang, dan
/// yang menyimpang di sini adalah jejak asal angka.
fn prior_result(plan: &Plan, response: &SettledResponse, row_count: usize) -> MemoryFact {
    let provenance = response
        .blocks
        .as_array()
        .and_then(|blocks| {
            blocks
                .iter()
                .find(|block| block.get("type") == Some(&Value::String("provenance".into())))
        })
        .cloned()
        .unwrap_or_else(|| json!({}));

    MemoryFact {
        kind: "PriorResult",
        entity_key: None,
        label: Some(plan.capability_id.clone()),
        fact_json: json!({
            "capability_id": plan.capability_id,
            "query_id": plan.query_id,
            "outcome": response.outcome,
            // I4 — nol baris berarti nol baris; "tidak diketahui" adalah
            // keadaan lain dan tidak pernah lewat jalur ini.
            "row_count": row_count,
        }),
        completeness: response.completeness.to_string(),
        completeness_reason: response.completeness_reason.clone(),
        provenance_json: provenance,
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use uuid::Uuid;

    use super::*;

    fn plan() -> Plan {
        Plan {
            capability_id: "savings_balance_summary".into(),
            query_id: "savings.balance_summary".into(),
            sql: "SELECT 1".into(),
            sql_file: "queries/savings/balance_summary.sql".into(),
            parameters: vec![
                Bound::Date(NaiveDate::from_ymd_opt(2025, 9, 15).unwrap()),
                Bound::OfficeIds(vec![1, 2, 3]),
                Bound::NullText,
            ],
            parameter_names: vec!["as_of_date".into(), "office_ids".into(), "product".into()],
            output_fields: vec!["total_balance".into()],
            output_sensitivity: vec!["public_business".into()],
            timeout_ms: 3000,
            catalog_version_id: Uuid::nil(),
            catalog_content_hash: "hash".into(),
            retrieval_score: 0.5,
            graph_json: json!({}),
            graph_hash: "graph".into(),
        }
    }

    fn response() -> SettledResponse {
        SettledResponse {
            kind: "analysis",
            outcome: "Answered",
            completeness: "Complete",
            completeness_reason: "curated_query:savings.balance_summary".into(),
            blocks: json!([
                { "type": "metrics", "metrics": [] },
                { "type": "provenance", "query_id": "savings.balance_summary" },
            ]),
            response_hash: "abc".into(),
        }
    }

    fn answer(provenance: &str, label: Option<&str>) -> AnsweredIdentity {
        AnsweredIdentity {
            field_id: "client_id".into(),
            value: "42".into(),
            label: label.map(str::to_string),
            provenance: provenance.into(),
            resolver_ref: Some("client.client_candidates".into()),
        }
    }

    #[test]
    fn result_is_promoted_last_so_it_carries_the_highest_seq() {
        let facts = promoted(&plan(), &response(), &[], 1);

        let kinds: Vec<&str> = facts.iter().map(|fact| fact.kind).collect();
        assert_eq!(kinds, vec!["ActiveScope", "PriorResult"]);
    }

    #[test]
    fn scope_keeps_the_office_list_and_the_bound_period() {
        let facts = promoted(&plan(), &response(), &[], 1);
        let scope = &facts[0].fact_json;

        assert_eq!(scope["office_ids"], json!([1, 2, 3]));
        assert_eq!(scope["period"]["as_of_date"], "2025-09-15");
        // Parameter opsional yang tidak diisi bukan bagian dari scope.
        assert!(scope["period"].get("product").is_none());
    }

    #[test]
    fn prior_result_reuses_the_provenance_block_of_the_response() {
        let facts = promoted(&plan(), &response(), &[], 7);
        let result = facts.last().unwrap();

        assert_eq!(result.provenance_json["type"], "provenance");
        assert_eq!(result.fact_json["row_count"], 7);
        assert_eq!(result.completeness, "Complete");
    }

    #[test]
    fn auto_bound_identity_keeps_its_provenance_and_is_not_called_confirmed() {
        let facts = promoted(
            &plan(),
            &response(),
            &[answer("resolver_unique", Some("Siti"))],
            1,
        );

        let entity = &facts[0];
        assert_eq!(entity.kind, "ResolvedEntity");
        assert_eq!(entity.entity_key.as_deref(), Some("client_id"));
        assert_eq!(entity.label.as_deref(), Some("Siti"));
        assert_eq!(entity.fact_json["provenance"], "resolver_unique");
    }

    #[test]
    fn confirmed_identity_is_not_labelled_auto_bound() {
        let facts = promoted(&plan(), &response(), &[answer("user_confirmed", None)], 1);

        assert_eq!(facts[0].fact_json["provenance"], "user_confirmed");
        assert!(facts[0].label.is_none());
    }
}
