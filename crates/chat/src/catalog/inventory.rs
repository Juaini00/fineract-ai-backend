//! Parser untuk `docs/data/dataset-inventory.md` — sumber kebenaran status
//! setiap baris (`inherited` / `gap` / `deferred-onboarding` / `excluded` /
//! `cross-reference` / `summary`), dibaca sungguhan dari dokumen, bukan
//! diduplikasi sebagai konstanta di `validate.rs` (FIN-153, permintaan
//! reviewer: "do not duplicate arbitrary status constants").
//!
//! Hanya tabel per-baris yang diparse — dikenali dari header yang diakhiri
//! `| Confidence | Status |`, ciri yang tidak dimiliki tabel lain di dokumen
//! ini (matriks lintas-domain §1, matriks cakupan §12, bukti lokal §11).
//! Tabel `| --- | ... |` pemisah dan header itu sendiri dilewati.
//!
//! ID pada beberapa baris memuat anotasi, mis. `LOAN-12 (D15)` atau
//! `FDRD-8 (RD)` — bagian dalam kurung dibuang, hanya id murni yang disimpan.

use std::collections::BTreeMap;

/// Kosakata status yang dideklarasikan dokumen §0 ("Status vocabulary used in
/// every table"). Beberapa sel status memuat anotasi setelah katanya, mis.
/// `"gap (savings-only inherited; loan and share account rosters have no
/// capability)"` — kategorinya tetap `gap`, anotasi hanya prosa tambahan.
pub const KNOWN_STATUSES: &[&str] = &[
    "inherited",
    "gap",
    "deferred-onboarding",
    "excluded",
    "cross-reference",
    "summary",
];

/// Normalisasi sel status mentah ke salah satu [`KNOWN_STATUSES`] dengan
/// mencocokkan prefiks — anotasi dalam kurung dibuang. `None` bila sel itu
/// tidak dimulai dengan kosakata yang dikenal sama sekali (dianggap tidak
/// terparse, bukan ditebak).
pub fn category(raw: &str) -> Option<&'static str> {
    KNOWN_STATUSES.iter().find(|&&known| raw.starts_with(known)).copied()
}

/// Ambang jumlah baris minimal yang dianggap sehat. Dokumen sungguhan
/// menghasilkan ratusan baris (lihat `parses_the_real_dataset_inventory_document`);
/// bila header tabel berubah dan parser diam-diam gagal, hasilnya jauh di
/// bawah ini — pemanggil (`loader::load`) memakai ini untuk gagal keras
/// (`unreadable`), bukan lolos dengan peta kosong/sebagian (FIN-153,
/// permintaan reviewer: "parser harus fail closed").
pub const MIN_EXPECTED_ROWS: usize = 50;

/// `id` baris (mis. `ORG-1`, `D12`, `D15a`) → kategori status ternormalisasi.
pub fn parse(markdown: &str) -> BTreeMap<String, String> {
    let mut rows = BTreeMap::new();
    let mut in_target_table = false;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            in_target_table = false;
            continue;
        }

        if trimmed.ends_with("| Confidence | Status |") {
            in_target_table = true;
            continue;
        }

        if !in_target_table {
            continue;
        }

        if is_separator_row(trimmed) {
            continue;
        }

        let cells: Vec<&str> = trimmed
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(str::trim)
            .collect();

        let (Some(id_cell), Some(status_cell)) = (cells.first(), cells.last()) else {
            continue;
        };

        let id = id_cell
            .split(" (")
            .next()
            .unwrap_or(id_cell)
            .trim()
            .to_string();

        let Some(status) = category(status_cell) else {
            continue;
        };

        if id.is_empty() {
            continue;
        }

        rows.insert(id, status.to_string());
    }

    rows
}

fn is_separator_row(line: &str) -> bool {
    line.chars().all(|c| matches!(c, '|' | '-' | ' ' | ':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_plain_row_from_a_target_table() {
        let markdown = "\
| Tag | Rule | Owning section |
| --- | --- | --- |
| XR-CUR | Totals are per currency | somewhere |

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability (Mode-1) | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ORG-1 | Manage Offices | t | f | r | o | ti | e | a | c | known | inherited |
| ORG-2 | Manage Holidays | t | f | r | o | ti | e | a | c | known | gap |
";
        let rows = parse(markdown);
        assert_eq!(rows.get("ORG-1"), Some(&"inherited".to_string()));
        assert_eq!(rows.get("ORG-2"), Some(&"gap".to_string()));
        // Tabel lain (bukan header Confidence|Status) tidak ikut terparse.
        assert!(!rows.contains_key("XR-CUR"));
    }

    #[test]
    fn strips_parenthetical_annotation_from_the_id_cell() {
        let markdown = "\
| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| LOAN-12 (D15) | Capitalized-income | t | f | r | o | ti | e | a | c | known | gap |
| FDRD-8 (RD) | Contributions | t | f | r | o | ti | e | a | c | known | gap |
";
        let rows = parse(markdown);
        assert_eq!(rows.get("LOAN-12"), Some(&"gap".to_string()));
        assert_eq!(rows.get("FDRD-8"), Some(&"gap".to_string()));
    }

    #[test]
    fn a_table_that_ends_before_confidence_status_is_ignored() {
        let markdown = "\
| Status | Meaning |
| --- | --- |
| inherited | Requirement is met by an existing, approved Mode-1 capability today. |
";
        let rows = parse(markdown);
        assert!(rows.is_empty());
    }

    /// Terhadap dokumen sungguhan — bukti bahwa parser ini benar-benar
    /// membaca `docs/data/dataset-inventory.md`, bukan hanya fixture di atas.
    #[test]
    fn parses_the_real_dataset_inventory_document() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/data/dataset-inventory.md");
        let markdown = std::fs::read_to_string(&path).expect("dataset-inventory.md terbaca");
        let rows = parse(&markdown);

        assert_eq!(rows.get("ORG-1").map(String::as_str), Some("inherited"));
        assert_eq!(rows.get("CLI-5").map(String::as_str), Some("gap"));
        assert_eq!(rows.get("CLI-6").map(String::as_str), Some("gap"));
        assert_eq!(rows.get("FDRD-8").map(String::as_str), Some("gap"));
        assert_eq!(rows.get("D09").map(String::as_str), Some("gap"));
        assert_eq!(rows.get("D12").map(String::as_str), Some("gap"));
        assert_eq!(rows.get("D13").map(String::as_str), Some("excluded"));
        assert_eq!(rows.get("D15a").map(String::as_str), Some("deferred-onboarding"));
        assert_eq!(rows.get("D15b").map(String::as_str), Some("deferred-onboarding"));
        assert_eq!(rows.get("D15d").map(String::as_str), Some("gap"));
        assert_eq!(rows.get("D15e").map(String::as_str), Some("gap"));
        assert!(!rows.contains_key("FAKE-999"));
        assert!(rows.len() > 50, "expected the real document to yield many rows, got {}", rows.len());
    }

    #[test]
    fn unknown_id_is_simply_absent_not_a_false_match() {
        let markdown = "\
| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ORG-1 | Manage Offices | t | f | r | o | ti | e | a | c | known | inherited |
";
        let rows = parse(markdown);
        assert!(!rows.contains_key("FAKE-999"));
    }
}
