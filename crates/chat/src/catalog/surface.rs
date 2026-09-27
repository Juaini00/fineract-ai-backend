//! Permukaan yang tidak disetujui (OVR-6.6, FIN-139).
//!
//! `policies/unsupported_requests.yaml` menolak keras (`hard_reject`) permintaan
//! atas field rahasia, tabel di luar cakupan, dan aksi yang tidak didukung, dan
//! check `hard_reject_maps_to_policy` melarang kasus itu dipetakan ke
//! capability yang disetujui. Tanpa guard ini retrieval memetakannya ke
//! capability baca terdekat: "Show the password hash of every app user."
//! dijawab `Answered`/`Complete` lewat `client_list_recent`.
//!
//! Kosakatanya dibaca dari knowledge, tidak ditanam di Rust:
//!
//! - **Field rahasia** — `examples` kelas `secret_never_expose` di
//!   `schema/fineract/columns/sensitivity.yaml` ("request secret fields",
//!   "command JSON").
//! - **Tabel di luar cakupan** — `excluded_tables` setiap
//!   `data-scope/areas/*.yaml` ("request out-of-scope tables"). Area milik
//!   domain berstatus `deferred` (loans, accounting_gl, tax) dilewati: domain
//!   itu sendiri menetapkan jawabannya `Unsupported` dengan alasan deferred
//!   (`domains/loan.yaml` `default_rules`), bukan penolakan kebijakan.
//! - **Intent yang tidak didukung** — `unsupported_intents` setiap
//!   `domains/*.yaml`.
//!
//! Domain berstatus `deferred` (loans, tax, accounting_gl) punya guard
//! terpisah, [`DeferredDomains`] (FIN-140): pertanyaan baca atas subjeknya
//! ("Show loan transactions.", "Tampilkan transaksi pinjaman bulan ini.")
//! bukan penolakan kebijakan, jadi tidak masuk kosakata [`Surfaces`] di atas
//! — outcome-nya `Unsupported`, bukan `BlockedByPolicy`. Tanpa guard ini
//! pertanyaan itu jatuh ke capability baca terdekat (mis. `savings_*`) dan
//! dijawab `Answered` dengan data yang salah subjeknya.
//!
//! Sengaja sempit — pertanyaan baca atas subjek yang disetujui yang ikut
//! tertolak adalah bug yang sama nyatanya:
//!
//! 1. Istilah dicocokkan sebagai frasa token utuh yang berurutan (huruf kecil,
//!    bentuk jamak `-s` dinormalisasi), tidak pernah sebagai substring.
//! 2. Nama tabel satu segmen (`m_role`, `m_permission`, `m_appuser`) tidak
//!    pernah cocok sebagai kata lepas — "permission" dan "role" adalah kata
//!    biasa — hanya sebagai identifier (`m_role`) atau kata majemuk yang
//!    dipisah (`app user` → `appuser`).
//! 3. Istilah yang juga muncul di prosa capability (display_name, description,
//!    examples, supported_intents) dibuang: kata yang dipakai permukaan yang
//!    disetujui sendiri bukan bukti permukaan yang tidak disetujui (contoh:
//!    `result`, yang muncul di deskripsi `client.name_lookup`).

use std::collections::BTreeSet;

use crate::catalog::model::{Capability, DataScopeArea, Domain};

/// Kosakata permukaan yang tidak disetujui.
#[derive(Debug, Clone, Default)]
pub struct Surfaces {
    terms: Vec<Term>,
}

/// Satu istilah yang, bila disebut, menandai permintaan atas permukaan yang
/// tidak disetujui.
#[derive(Debug, Clone)]
pub struct Term {
    /// Asal istilah di knowledge — untuk log, bukan untuk response: response
    /// tidak boleh membocorkan schema yang dibatasi (PRD §7).
    pub source: &'static str,
    /// Nama seperti tertulis di knowledge.
    pub name: String,
    /// Frasa token yang cocok bila muncul berurutan di permintaan.
    phrases: Vec<Vec<String>>,
    /// Kata majemuk yang cocok bila ditulis sebagai dua token bersebelahan.
    compound: Option<String>,
}

impl Surfaces {
    pub fn build<'a>(
        secret_fields: &[String],
        areas: &[DataScopeArea],
        domains: &[Domain],
        capabilities: impl Iterator<Item = &'a Capability>,
    ) -> Self {
        let deferred_areas: BTreeSet<&str> = domains
            .iter()
            .filter(|domain| is_uncontracted(domain))
            .flat_map(|domain| domain.data_areas.iter().map(String::as_str))
            .collect();

        let mut terms: Vec<Term> = secret_fields
            .iter()
            .map(|field| Term::secret_field(field))
            .collect();

        terms.extend(
            areas
                .iter()
                .filter(|area| !deferred_areas.contains(area.id.as_str()))
                .flat_map(|area| area.excluded_tables.iter())
                .map(|table| Term::excluded_table(table)),
        );

        terms.extend(
            domains
                .iter()
                .flat_map(|domain| domain.unsupported_intents.iter())
                .map(|intent| Term::unsupported_intent(intent.phrase())),
        );

        let approved: Vec<Vec<String>> = capabilities
            .flat_map(|capability| {
                capability
                    .display_name
                    .iter()
                    .chain(capability.description.iter())
                    .chain(capability.examples.iter())
                    .chain(capability.supported_intents.iter())
            })
            .map(|text| tokens(text))
            .collect();

        terms.retain(|term| !approved.iter().any(|text| term.occurs_in(text)));
        terms.sort_by(|left, right| left.name.cmp(&right.name));
        terms.dedup_by(|left, right| left.name == right.name);

        Self { terms }
    }

    /// Istilah pertama yang disebut `text`, bila ada.
    pub fn find(&self, text: &str) -> Option<&Term> {
        let tokens = tokens(text);
        self.terms.iter().find(|term| term.occurs_in(&tokens))
    }
}

/// Domain berstatus `deferred` (deferred ke onboarding deployment, mis. D10)
/// atau `gap` (cakupan disepakati §1/D01–D15 tetapi belum punya capability
/// disetujui, mis. loan, tax, accounting, audit — FIN-153) — FIN-140,
/// `default_rules` masing-masing: "respond as unsupported with a
/// deferred/gap reason". Beda dari [`Surfaces`]: ini bukan penolakan
/// kebijakan, jadi outcome-nya `Unsupported`, bukan `BlockedByPolicy`.
///
/// Kosakatanya dua sumber: `concepts[].synonyms` domain uncontracted sendiri
/// (subjek utuh domain, EN + ID bercampur, konvensi yang sama dipakai
/// `savings.yaml`, `loan.yaml`, dst), dan `gap_intents` **setiap** domain
/// (termasuk domain `approved_mvp`/`candidate` — satu intent spesifik boleh
/// gap sementara domainnya sendiri sebagian sudah punya capability, mis.
/// alamat client atau ringkasan akun group di luar savings, FIN-153).
/// Istilah yang juga menjadi synonym konsep domain yang TIDAK uncontracted
/// dibuang — "credit" adalah synonym `loan` (gap) sekaligus synonym
/// `deposit` di `savings.yaml` (approved_mvp), jadi ia tidak boleh memicu
/// guard ini dan membisukan pertanyaan savings yang sah.
#[derive(Debug, Clone, Default)]
pub struct DeferredDomains {
    terms: Vec<Term>,
}

impl DeferredDomains {
    pub fn build<'a>(
        domains: &[Domain],
        capabilities: impl Iterator<Item = &'a Capability>,
    ) -> Self {
        let mut excluded: Vec<Vec<String>> = capabilities
            .flat_map(|capability| {
                capability
                    .display_name
                    .iter()
                    .chain(capability.description.iter())
                    .chain(capability.examples.iter())
                    .chain(capability.supported_intents.iter())
            })
            .map(|text| tokens(text))
            .collect();

        excluded.extend(
            domains
                .iter()
                .filter(|domain| !is_uncontracted(domain))
                .flat_map(|domain| domain.concepts.iter())
                .flat_map(|concept| concept.synonyms.iter())
                .map(|synonym| tokens(synonym)),
        );

        let mut terms: Vec<Term> = domains
            .iter()
            .filter(|domain| is_uncontracted(domain))
            .flat_map(|domain| domain.concepts.iter().map(move |concept| (domain, concept)))
            .flat_map(|(domain, concept)| {
                concept
                    .synonyms
                    .iter()
                    .map(move |synonym| Term::phrase(reason_for(domain), synonym))
            })
            .collect();

        terms.extend(domains.iter().flat_map(|domain| {
            domain
                .gap_intents
                .iter()
                .map(move |intent| Term::phrase(reason_for(domain), intent.phrase()))
        }));

        terms.retain(|term| !excluded.iter().any(|text| term.occurs_in(text)));
        terms.sort_by(|left, right| left.name.cmp(&right.name));
        terms.dedup_by(|left, right| left.name == right.name);

        Self { terms }
    }

    /// Istilah pertama yang menandai subjek domain deferred/gap di `text`, bila ada.
    pub fn find(&self, text: &str) -> Option<&Term> {
        let tokens = tokens(text);
        self.terms.iter().find(|term| term.occurs_in(&tokens))
    }
}

/// Domain tanpa capability disetujui secara umum: `deferred`
/// (onboarding-dependent, D10), `gap` (cakupan disepakati, belum dikontrak —
/// FIN-153), atau `candidate` (mis. group/center — hanya aktif bila
/// deployment memverifikasi area yang mendasarinya; default_rules-nya sendiri
/// menuntut "respond as unsupported" selama belum diverifikasi). Ketiganya
/// dijawab `Unsupported`, bukan `BlockedByPolicy`.
fn is_uncontracted(domain: &Domain) -> bool {
    matches!(
        domain.status.as_deref(),
        Some("deferred") | Some("gap") | Some("candidate")
    )
}

/// `completeness_reason` untuk [`DeferredDomains`] — tiga varian, bukan satu
/// (FIN-153): "belum dikontrak" (`gap`) tidak sama dengan "menunggu
/// deployment" (`deferred`, D10/D15a/b) atau "area belum diverifikasi
/// deployment" (`candidate`, mis. group/center). `gap_intents` sebuah domain
/// yang statusnya sendiri bukan salah satu dari ini (mis. `approved_mvp`,
/// misal alamat client) jatuh ke lengan default `domain_gap` — intent itu
/// sendiri tetap agreed-tapi-uncontracted meski domain induknya sebagian
/// sudah disetujui.
fn reason_for(domain: &Domain) -> &'static str {
    match domain.status.as_deref() {
        Some("deferred") => "domain_deferred",
        Some("candidate") => "domain_conditional_not_enabled",
        _ => "domain_gap",
    }
}

impl Term {
    /// Istilah frasa polos: majemuk bila satu kata, phrase bila lebih.
    fn phrase(source: &'static str, name: &str) -> Self {
        let segments = tokens(name);
        Self {
            source,
            name: name.to_string(),
            compound: (segments.len() == 1).then(|| segments[0].clone()),
            phrases: vec![segments],
        }
    }

    fn secret_field(name: &str) -> Self {
        Self::phrase("secret_never_expose", name)
    }

    fn excluded_table(name: &str) -> Self {
        let segments = tokens(name);
        // Prefiks tabel Fineract satu huruf (`m_`, `x_`) tidak diucapkan.
        let bare = match segments.split_first() {
            Some((prefix, rest)) if prefix.chars().count() == 1 && !rest.is_empty() => {
                rest.to_vec()
            }
            _ => segments.clone(),
        };

        let (phrases, compound) = if bare.len() == 1 {
            // Satu kata: hanya identifier utuh atau majemuk yang dipisah.
            (vec![segments], Some(bare[0].clone()))
        } else if bare == segments {
            (vec![segments], None)
        } else {
            (vec![segments, bare], None)
        };

        Self {
            source: "excluded_tables",
            name: name.to_string(),
            phrases,
            compound,
        }
    }

    fn unsupported_intent(intent: &str) -> Self {
        let mut phrase = tokens(intent);
        // "identity document reporting": yang ditolak adalah subjeknya;
        // "reporting" adalah kegiatannya, bukan bagian dari istilah.
        if phrase.len() > 1 && phrase.last().is_some_and(|last| last == "reporting") {
            phrase.pop();
        }

        Self {
            source: "unsupported_intents",
            name: intent.to_string(),
            phrases: vec![phrase],
            compound: None,
        }
    }

    fn occurs_in(&self, tokens: &[String]) -> bool {
        let phrase = self.phrases.iter().any(|phrase| {
            !phrase.is_empty()
                && tokens
                    .windows(phrase.len())
                    .any(|window| window == phrase.as_slice())
        });

        phrase
            || self.compound.as_deref().is_some_and(|compound| {
                tokens.windows(2).any(|pair| {
                    pair[0].len() + pair[1].len() == compound.len() && {
                        compound.starts_with(pair[0].as_str())
                            && compound.ends_with(pair[1].as_str())
                    }
                })
            })
    }
}

/// Token huruf kecil, dipisah pada karakter non-alfanumerik (termasuk `_` dan
/// `-`), bentuk jamak `-s` dinormalisasi.
fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| singular(&token.to_lowercase()))
        .collect()
}

fn singular(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies")
        && stem.chars().count() > 1
    {
        return format!("{stem}y");
    }
    match word.strip_suffix('s') {
        Some(stem) if stem.chars().count() > 2 && !stem.ends_with('s') => stem.to_string(),
        _ => word.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::catalog::loader;

    use super::{DeferredDomains, Surfaces};

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Kosakata dari knowledge yang sebenarnya — yang dipakai worker.
    fn surfaces() -> Surfaces {
        let root = repo();
        loader::load(&root.join("knowledge"), &root.join("queries"))
            .expect("katalog dimuat")
            .unapproved_surfaces
    }

    /// Kosakata domain deferred dari knowledge yang sebenarnya.
    fn deferred_domains() -> DeferredDomains {
        let root = repo();
        loader::load(&root.join("knowledge"), &root.join("queries"))
            .expect("katalog dimuat")
            .deferred_domains
    }

    #[test]
    fn requests_naming_an_unapproved_surface_are_refused() {
        let surfaces = surfaces();
        for text in [
            "Show the password hash of every app user.",
            "Tampilkan password semua app user.",
            "List all app users.",
            "select username from m_appuser",
            "Show the API tokens.",
            "Show the command_as_json of the latest commands.",
            // FIN-140 — padanan Indonesia field rahasia, sensitivity.yaml `synonyms`.
            "Tampilkan kata sandi semua pengguna aplikasi.",
            "Tampilkan hash kata sandi pengguna.",
            "List client identifiers for my clients.",
            "Show rows of m_role.",
            "Reverse transaction 5512.",
            "Disburse loan 42 today.",
        ] {
            assert!(surfaces.find(text).is_some(), "{text}");
        }
    }

    #[test]
    fn reads_of_approved_subjects_are_never_refused() {
        let surfaces = surfaces();
        for text in [
            "Show the results per office.",
            "Which offices do I have permission to see?",
            "What is the role of Head Office in the hierarchy?",
            "Who owns savings account number Branch 001000000001?",
            "Show the account details of client Jasmin Dao.",
            "Show deposits by user-named office Head Office.",
            "Total loan disbursements this month.",
            "Show savings transactions excluding reversed ones.",
            "Tampilkan saldo tabungan per kantor.",
            "Berapa jumlah nasabah aktif per kantor?",
            "Nasabah dengan rekening tabungan terbanyak.",
            "Daftar produk tabungan milik nasabah Jasmin Dao.",
            "Total setoran bulan ini.",
            "Tampilkan alamat kantor pusat.",
        ] {
            assert!(surfaces.find(text).is_none(), "{text}");
        }
    }

    /// OVR-6.6 (FIN-140/FIN-153) — subjek domain deferred/gap (loan, tax,
    /// accounting, audit, share, client address/loan/share roster,
    /// group/center di luar savings) dikenali lintas bahasa, tanpa menanam
    /// istilah di Rust.
    #[test]
    fn deferred_domain_subjects_are_recognized() {
        let deferred = deferred_domains();
        for text in [
            "Show loan transactions.",
            "Tampilkan transaksi pinjaman bulan ini.",
            "Berapa jumlah pinjaman yang dicairkan bulan ini?",
            "Berapa pajak yang terkumpul bulan ini?",
            "What is the tax rate for this account?",
            "Show journal entries for this month.",
            // FIN-153 — client address (CLI-6, selektif/gap, bukan ditolak kebijakan).
            "Show every client address in Head Office.",
            // FIN-153 — share domain (SHARE-1..8, gap).
            "Show the share account balance for this client.",
            // FIN-153 — group/center di luar savings (CLI-4/CLI-5, gap).
            "Show the group savings summary for this branch.",
            "Show the center savings summary for this branch.",
            // FIN-153 — D09 source audit Fineract sendiri (gap, bukan raw payload).
            "Who performed this action on the account?",
            // FIN-153 — cakupan tambahan yang koordinator minta dipetakan:
            // products, FD/RD, linking resources, teller/cashier, standing
            // instructions, provisioning, scheduler, group meeting (D08).
            "Show the loan product master list.",
            "Show the recurring deposit contribution for this account.",
            "Show the actual account transfer between these two accounts.",
            "Show the teller assignment for this branch.",
            "Show the standing instruction execution history for this account.",
            "Show the provisioning result history for this loan.",
            "Show the batch job status for interest posting last night.",
            "Show the group meeting attendance for this center.",
            // FIN-153 (second review pass) — D04 client charge, D10 custom
            // datatable, D15a/b survey/credit bureau, D15d family member,
            // D15e inter-office cash, and D01/D02/D03/D11 narrow phrases.
            "Show the client charge for this account.",
            "Show the custom datatable EmploymentDetails for this client.",
            "Show the credit bureau report for this client.",
            "Show the family member detail for this client.",
            "Show the inter-office cash transaction between these branches.",
            "Reproduce the published report for last month.",
            "Show the historical office as of a past date for this client.",
            "Show the consolidated total across currencies for this branch.",
            "Show the completeness analysis by category for this portfolio.",
            "Tampilkan saldo akun buku besar.",
        ] {
            assert!(deferred.find(text).is_some(), "{text}");
        }
    }

    /// "credit" adalah synonym konsep `loan` (deferred) SEKALIGUS synonym
    /// konsep `deposit` di `savings.yaml` (approved_mvp) — istilah ambigu
    /// begini harus dibuang dari kosakata deferred (FIN-140), bukan
    /// membisukan pertanyaan savings yang sah.
    #[test]
    fn ambiguous_terms_shared_with_an_approved_domain_never_trigger_the_deferred_guard() {
        let deferred = deferred_domains();
        for text in [
            "Show savings credit transactions this month.",
            "Total setoran (kredit) tabungan bulan ini.",
            "Show the results per office.",
        ] {
            assert!(deferred.find(text).is_none(), "{text}");
        }
    }

    /// Setiap `request_text` di koleksi Bruno adalah pertanyaan yang harus
    /// sampai ke retrieval — kecuali rangkaian `policy-surface-*`, yang
    /// justru membuktikan penolakannya.
    #[test]
    fn bruno_request_texts_are_refused_only_in_the_surface_chain() {
        let surfaces = surfaces();
        let mut files = Vec::new();
        collect_yml(&repo().join("fineract-assistant-api"), &mut files);

        let mut seen = 0;
        for path in files {
            let text = std::fs::read_to_string(&path).expect("berkas Bruno terbaca");
            let expect_refusal = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("policy-surface-"));

            for (index, _) in text.match_indices("\"request_text\":") {
                let rest = text[index + "\"request_text\":".len()..].trim_start();
                let request: String = serde_json::Deserializer::from_str(rest)
                    .into_iter::<String>()
                    .next()
                    .expect("request_text berisi string")
                    .expect("string JSON sah");
                seen += 1;
                assert_eq!(
                    surfaces.find(&request).is_some(),
                    expect_refusal,
                    "{}: {request}",
                    path.display()
                );
            }
        }
        assert!(seen > 0, "tidak ada request_text yang terbaca");
    }

    /// Sama seperti di atas, tapi untuk guard domain deferred (FIN-140):
    /// hanya rangkaian `policy-deferred-*` (kecuali variannya `-control-`,
    /// yang justru membuktikan istilah ambigu TIDAK ikut tertolak) yang
    /// boleh tertangkap.
    #[test]
    fn bruno_request_texts_are_refused_only_in_the_deferred_chain() {
        let deferred = deferred_domains();
        let mut files = Vec::new();
        collect_yml(&repo().join("fineract-assistant-api"), &mut files);

        let mut seen = 0;
        for path in files {
            let text = std::fs::read_to_string(&path).expect("berkas Bruno terbaca");
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let expect_refusal =
                name.starts_with("policy-deferred-") && !name.contains("-control-");

            for (index, _) in text.match_indices("\"request_text\":") {
                let rest = text[index + "\"request_text\":".len()..].trim_start();
                let request: String = serde_json::Deserializer::from_str(rest)
                    .into_iter::<String>()
                    .next()
                    .expect("request_text berisi string")
                    .expect("string JSON sah");
                seen += 1;
                assert_eq!(
                    deferred.find(&request).is_some(),
                    expect_refusal,
                    "{}: {request}",
                    path.display()
                );
            }
        }
        assert!(seen > 0, "tidak ada request_text yang terbaca");
    }

    /// FIN-153 — "belum dikontrak" (gap) tidak sama dengan "menunggu
    /// deployment" (deferred) atau "area belum diverifikasi" (candidate);
    /// masing-masing menghasilkan `completeness_reason` sendiri, dibaca dari
    /// `term.source` di worker (bukan satu konstanta tunggal seperti
    /// sebelumnya).
    #[test]
    fn reason_distinguishes_gap_deferred_and_conditional() {
        let deferred = deferred_domains();

        let gap = deferred.find("Show loan transactions.").expect("loan cocok");
        assert_eq!(gap.source, "domain_gap");

        let conditional = deferred
            .find("Show the group savings summary for this branch.")
            .expect("group cocok");
        assert_eq!(conditional.source, "domain_conditional_not_enabled");

        // Intent gap di dalam domain approved_mvp (alamat client) tetap
        // `domain_gap`, bukan reason domain induknya.
        let client_gap = deferred
            .find("Show every client address in Head Office.")
            .expect("alamat client cocok");
        assert_eq!(client_gap.source, "domain_gap");
    }

    fn collect_yml(directory: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).expect("direktori Bruno terbaca") {
            let path = entry.expect("entri direktori").path();
            if path.is_dir() {
                collect_yml(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "yml") {
                found.push(path);
            }
        }
    }
}
