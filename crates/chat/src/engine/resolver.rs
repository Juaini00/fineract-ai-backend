//! Resolver opsi untuk slot yang tidak boleh diikat dari teks bebas (K1).
//!
//! Aturan yang dijaga modul ini:
//!
//! 1. **Sumber opsi adalah query yang disetujui katalog.** Slot menyebut
//!    `probe: { dataset_id, shape_id }`, shape itu ditautkan manifest lewat
//!    `resolves:`, dan manifest itulah yang menyediakan SQL-nya. Tidak ada
//!    pencarian yang dikarang di sini.
//! 2. **Scope selalu dari otorisasi.** Seluruh parameter resolver wajib
//!    bersumber `authorized_scope`; validator katalog menolak yang lain.
//! 3. **Label tunduk pada sakelar PII dan fail closed.** Saat PII mati, nama
//!    nasabah tidak menjadi label — `label_fallback` katalog yang dipakai.
//!    `masked_output` (mis. nomor rekening yang sudah dipotong) BUKAN PII —
//!    ia tetap boleh menjadi label/atribut apa pun keadaan sakelarnya
//!    (columns/sensitivity.yaml: sama-sama `allow_if_capability_declares`
//!    dengan `public_business`, berbeda dari `pii`).
//! 4. **Pemotongan dinyatakan.** Kandidat melebihi `RESOLVER_MAX_CANDIDATES`
//!    menjadi `truncated: true`, bukan daftar yang diam-diam lebih pendek (I5).

use serde_json::{Map, Value};

use foundation::db::FineractDb;

use crate::{
    catalog::{loader::Catalog, model::ProbeRef},
    engine::{
        executor::{self, ExecutionError},
        planner::Bound,
    },
};

/// Resolver yang sudah diikat ke sebuah slot parameter, siap dijalankan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverSlot {
    pub dataset_id: String,
    pub shape_id: String,
    pub query_id: String,
    pub sql: String,
    pub timeout_ms: u64,
    /// Kolom yang menjadi `option_id` (identitas opsi yang diterbitkan).
    pub option_field: String,
    /// Kolom yang nilainya menjadi binding slot.
    pub binding_field: String,
    /// Tipe parameter pada query utama; binding wajib memenuhinya.
    pub binding_kind: String,
    pub label_fields: Vec<String>,
    pub label_fallback: Option<String>,
    /// Kolom yang boleh ikut sebagai atribut/label tanpa sakelar PII:
    /// `public_business` dan `masked_output` (columns/sensitivity.yaml —
    /// keduanya `allow_if_capability_declares`, berbeda dari `pii` yang
    /// menuntut sakelar menyala). Identifier mentah yang sudah dipotong
    /// (mis. `masked_account_number`) bukan PII; menahannya di belakang
    /// sakelar PII membuat resolver identitas non-klien (FIN-137) tidak
    /// pernah dapat mencari lewat label sama sekali.
    pub public_fields: Vec<String>,
    /// `row_cap` shape: ukuran halaman **default**, bukan batas populasi.
    /// Memperlakukannya sebagai batas populasi membuat kandidat ke-26 tidak
    /// pernah dapat dipilih walaupun paginasi ada.
    pub page_size: usize,
}

impl ResolverSlot {
    /// Susun dari katalog. `binding_kind` datang dari manifest query yang
    /// **memakai** slot, bukan dari resolver — nilainya harus dapat diikat ke
    /// parameter itu, bukan sekadar ada.
    pub fn from_catalog(catalog: &Catalog, probe: &ProbeRef, binding_kind: &str) -> Option<Self> {
        let resolver = catalog.resolver_for(&probe.dataset_id, &probe.shape_id)?;
        let entity = resolver.dataset.entity.as_ref()?;
        let sql_file = resolver.query.sql_file.as_deref()?;
        let sql = catalog.sql_files.get(sql_file)?;

        Some(Self {
            dataset_id: probe.dataset_id.clone(),
            shape_id: probe.shape_id.clone(),
            query_id: resolver.query.id.clone(),
            sql: sql.clone(),
            timeout_ms: resolver.query.timeout_ms.unwrap_or(3_000),
            option_field: entity.id_field.clone(),
            binding_field: probe.output_slot.clone(),
            binding_kind: binding_kind.to_string(),
            label_fields: entity.label_fields.clone(),
            label_fallback: entity.label_fallback.clone(),
            public_fields: resolver
                .query
                .output_fields
                .iter()
                .filter(|field| {
                    matches!(
                        field.sensitivity.as_deref(),
                        Some("public_business") | Some("masked_output")
                    )
                })
                .map(|field| field.name.clone())
                .collect(),
            // Shape tanpa row_cap: halaman probe yang lebih ketat, bukan tanpa batas.
            page_size: resolver.shape.row_cap.unwrap_or(25).max(1),
        })
    }
}

/// Satu opsi yang boleh diterbitkan kepada pengguna.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub option_id: String,
    /// Nilai yang akan menjadi binding bila opsi ini dipilih.
    pub binding: Value,
    pub label: String,
    pub attributes: Map<String, Value>,
}

/// Himpunan kandidat dalam scope, beserta pernyataan apakah ia dipotong.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidates {
    pub items: Vec<Candidate>,
    /// Jumlah baris yang benar-benar dikembalikan sumber sebelum pemotongan.
    pub matched_total: usize,
    pub truncated: bool,
}

/// Jalankan resolver terhadap Fineract dalam scope terotorisasi.
///
/// `search` menyaring **setelah** SQL yang disetujui berjalan: manifest resolver
/// tidak mendeklarasikan parameter pencarian, dan menambahkannya di sini berarti
/// mengarang predikat SQL. Penyaringan atas hasil yang sudah ter-scope tidak
/// memperluas apa pun.
///
/// ponytail: kandidat dimuat utuh lalu dipaginasi di memori, dibatasi
/// `RESOLVER_MAX_CANDIDATES`. Ceiling-nya O(n) per halaman atas satu tenant;
/// naikkan ke keyset SQL-side begitu ada resolver yang mendeklarasikan
/// parameter cursor-nya sendiri.
pub async fn candidates(
    fineract: &FineractDb,
    slot: &ResolverSlot,
    authorized_office_ids: &[i64],
    pii_enabled: bool,
    search: Option<&str>,
    max_candidates: usize,
) -> Result<Candidates, ExecutionError> {
    let executed = executor::run(
        fineract,
        &slot.sql,
        &[Bound::OfficeIds(authorized_office_ids.to_vec())],
        slot.timeout_ms,
    )
    .await?;

    Ok(select(
        slot,
        &executed.rows,
        pii_enabled,
        search,
        max_candidates,
    ))
}

/// Ubah baris resolver menjadi opsi. Dipisah dari IO supaya aturan label,
/// atribut dan pemotongan dapat diuji tanpa database.
pub fn select(
    slot: &ResolverSlot,
    rows: &[Map<String, Value>],
    pii_enabled: bool,
    search: Option<&str>,
    max_candidates: usize,
) -> Candidates {
    let needle = search
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase);

    let mut items: Vec<Candidate> = Vec::new();

    for row in rows {
        let (Some(option), Some(binding)) = (
            row.get(&slot.option_field).filter(|value| !value.is_null()),
            row.get(&slot.binding_field)
                .filter(|value| !value.is_null()),
        ) else {
            // Baris tanpa id atau tanpa nilai binding tidak dapat dipilih tanpa
            // menebak; ia dilewati, bukan diterbitkan sebagai opsi kosong.
            continue;
        };

        let label = label_for(slot, row, pii_enabled);

        if let Some(needle) = &needle
            && !label.to_lowercase().contains(needle.as_str())
        {
            continue;
        }

        items.push(Candidate {
            option_id: scalar_text(option),
            binding: binding.clone(),
            label,
            attributes: slot
                .public_fields
                .iter()
                .filter(|field| *field != &slot.option_field)
                .filter_map(|field| row.get(field).map(|value| (field.clone(), value.clone())))
                .collect(),
        });
    }

    // Urutan stabil, dan bukan urutan yang dikembalikan PostgreSQL: manifest
    // resolver tidak mendeklarasikan ORDER BY, jadi tanpa ini halaman kedua
    // dapat memuat ulang baris halaman pertama sementara baris lain tidak
    // pernah terlihat — paginasi yang kehilangan kandidat tanpa satu pun sinyal.
    items.sort_by(|left, right| {
        numeric(&left.option_id)
            .cmp(&numeric(&right.option_id))
            .then_with(|| left.option_id.cmp(&right.option_id))
    });

    let matched_total = items.len();
    let truncated = matched_total > max_candidates;
    items.truncate(max_candidates);

    Candidates {
        items,
        matched_total,
        truncated,
    }
}

/// Label opsi. Kolom PII hanya ikut bila sakelar PII menyala; bila tidak,
/// `label_fallback` katalog yang dipakai — fail closed, bukan label kosong.
fn label_for(slot: &ResolverSlot, row: &Map<String, Value>, pii_enabled: bool) -> String {
    if pii_enabled
        || slot
            .label_fields
            .iter()
            .all(|field| slot.public_fields.contains(field))
    {
        let parts: Vec<String> = slot
            .label_fields
            .iter()
            .filter_map(|field| row.get(field))
            .filter(|value| !value.is_null())
            .map(scalar_text)
            .filter(|text| !text.is_empty())
            .collect();

        if !parts.is_empty() {
            return parts.join(" — ");
        }
    }

    match &slot.label_fallback {
        Some(template) => interpolate(template, row),
        None => format!(
            "{} {}",
            slot.option_field,
            row.get(&slot.option_field)
                .map(scalar_text)
                .unwrap_or_default()
        ),
    }
}

/// Isi `{field}` pada `label_fallback` dari baris hasil. Placeholder yang tidak
/// ada di baris dibiarkan apa adanya — lebih baik terlihat daripada menjadi
/// string kosong yang tampak seperti label sah.
fn interpolate(template: &str, row: &Map<String, Value>) -> String {
    let mut output = template.to_string();
    for (key, value) in row {
        output = output.replace(&format!("{{{key}}}"), &scalar_text(value));
    }
    output
}

/// Id numerik diurutkan sebagai angka: "10" setelah "9", bukan sebelum "2".
fn numeric(option_id: &str) -> Option<i64> {
    option_id.parse().ok()
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Apakah sebuah binding memenuhi tipe parameter yang akan menerimanya.
pub fn binding_matches_kind(kind: &str, binding: &Value) -> bool {
    match kind {
        "integer" | "bigint" => {
            binding.is_i64()
                || binding
                    .as_str()
                    .is_some_and(|text| text.parse::<i64>().is_ok())
        }
        "string" => binding.is_string(),
        _ => false,
    }
}

/// Bentuk teks yang disimpan sebagai jawaban dan dibaca ulang planner.
pub fn binding_text(binding: &Value) -> String {
    scalar_text(binding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn slot() -> ResolverSlot {
        ResolverSlot {
            dataset_id: "client.identity".into(),
            shape_id: "identity_candidates".into(),
            query_id: "client.identity_resolve".into(),
            sql: String::new(),
            timeout_ms: 3_000,
            option_field: "client_id".into(),
            binding_field: "client_id".into(),
            binding_kind: "integer".into(),
            label_fields: vec!["client_display_name".into(), "office_name".into()],
            label_fallback: Some("Client {client_id}".into()),
            public_fields: vec!["client_id".into(), "office_id".into(), "office_name".into()],
            page_size: 2,
        }
    }

    fn row(id: i64, name: &str) -> Map<String, Value> {
        json!({
            "client_id": id,
            "client_display_name": name,
            "office_id": 1,
            "office_name": "Head Office",
        })
        .as_object()
        .cloned()
        .unwrap()
    }

    #[test]
    fn masked_output_label_shows_regardless_of_pii_switch() {
        // FIN-137: masked_account_number is `masked_output`, not `pii` — a
        // resolver like savings.accounts/identity_candidates must show it
        // (and let search match it) even when the caller lacks can_view_pii.
        let slot = ResolverSlot {
            dataset_id: "savings.accounts".into(),
            shape_id: "identity_candidates".into(),
            query_id: "savings.account_identity_candidates".into(),
            sql: String::new(),
            timeout_ms: 3_000,
            option_field: "savings_account_id".into(),
            binding_field: "savings_account_id".into(),
            binding_kind: "integer".into(),
            label_fields: vec!["masked_account_number".into()],
            label_fallback: Some("Savings account {savings_account_id}".into()),
            public_fields: vec!["savings_account_id".into(), "masked_account_number".into()],
            page_size: 25,
        };
        let rows = [
            json!({ "savings_account_id": 1, "masked_account_number": "****0001" })
                .as_object()
                .cloned()
                .unwrap(),
        ];

        let with_pii_off = select(&slot, &rows, false, None, 25);
        assert_eq!(with_pii_off.items[0].label, "****0001");

        let narrowed = select(&slot, &rows, false, Some("0001"), 25);
        assert_eq!(narrowed.items.len(), 1);
    }

    #[test]
    fn label_uses_pii_fields_only_when_the_switch_is_on() {
        let rows = [row(7, "Grace Dao")];

        assert_eq!(
            select(&slot(), &rows, true, None, 2).items[0].label,
            "Grace Dao — Head Office"
        );
        // Sakelar mati: nama tidak boleh menjadi label, dan label tidak boleh kosong.
        assert_eq!(
            select(&slot(), &rows, false, None, 2).items[0].label,
            "Client 7"
        );
    }

    #[test]
    fn option_id_and_binding_come_from_declared_columns() {
        let candidate = &select(&slot(), &[row(43, "Budi")], true, None, 2).items[0];

        assert_eq!(candidate.option_id, "43");
        assert_eq!(candidate.binding, json!(43));
        // Atribut hanya kolom public_business, dan bukan id-nya sendiri.
        assert!(!candidate.attributes.contains_key("client_display_name"));
        assert_eq!(candidate.attributes["office_name"], "Head Office");
    }

    #[test]
    fn candidate_order_is_stable_and_numeric() {
        // Urutan sumber sengaja acak: manifest resolver tidak punya ORDER BY.
        let rows = [row(10, "J"), row(2, "B"), row(9, "I")];
        let ids: Vec<String> = select(&slot(), &rows, true, None, 10)
            .items
            .into_iter()
            .map(|candidate| candidate.option_id)
            .collect();

        assert_eq!(ids, vec!["2", "9", "10"]);
    }

    #[test]
    fn truncation_is_declared_not_silent() {
        let rows = [row(1, "A"), row(2, "B"), row(3, "C")];
        let candidates = select(&slot(), &rows, true, None, 2);

        assert_eq!(candidates.items.len(), 2);
        assert_eq!(candidates.matched_total, 3);
        assert!(candidates.truncated);
    }

    #[test]
    fn search_narrows_the_issued_set_without_widening_scope() {
        let rows = [row(1, "Grace Dao"), row(2, "Budi Santoso")];
        let candidates = select(&slot(), &rows, true, Some("budi"), 2);

        assert_eq!(candidates.items.len(), 1);
        assert_eq!(candidates.items[0].option_id, "2");
    }

    #[test]
    fn rows_without_a_binding_value_are_never_issued_as_options() {
        let mut broken = row(9, "X");
        broken.insert("client_id".into(), Value::Null);

        assert!(select(&slot(), &[broken], true, None, 2).items.is_empty());
    }

    #[test]
    fn binding_type_is_checked_against_the_parameter_it_will_fill() {
        assert!(binding_matches_kind("integer", &json!(43)));
        assert!(binding_matches_kind("string", &json!("Head Office")));
        // Nama office tidak boleh mengisi slot bigint hanya karena ia ada.
        assert!(!binding_matches_kind("integer", &json!("Head Office")));
        assert!(!binding_matches_kind("string", &json!(43)));
    }
}
