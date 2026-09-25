//! Subcommand `retrieval-sweep` (FIN-142).
//!
//! Bukti corpus atas retrieval leksikal (`planner::lexical_candidate`) —
//! kode produksi yang sama dipakai `plan()`, bukan SQL yang diduplikasi.
//! Korpusnya: contoh `examples:` tiap capability manifest (harus memilih
//! dirinya sendiri), pasangan frasa→capability yang sudah diasersikan
//! `fineract-assistant-api/answers/**/*-job.yml` (nama job = "<capability_id>
//! job"), pasangan yang diasersikan `fineract-assistant-api/retrieval-selection/
//! *-response.yml`, dan held-out set FIN-142
//! (`crates/app/fixtures/fin142_held_out.json`).
//!
//! Arm vektor (semantic fallback) sengaja tidak diikutkan: ia hanya terpakai
//! saat arm leksikal tidak percaya diri, butuh `EMBEDDING_API_KEY` hidup, dan
//! residual FIN-142 murni fenomena arm leksikal (dua manifest bertetangga
//! dekat kosakata) — menyertakannya hanya menambah dependensi eksternal tanpa
//! menambah sinyal untuk sweep ini.

use std::fs;
use std::path::Path;

use chat::catalog::Catalog;
use chat::engine::planner;
use foundation::Foundation;
use serde::Deserialize;

struct CorpusCase {
    text: String,
    expected: String,
    source: String,
}

#[derive(Deserialize)]
struct JobFile {
    info: JobInfo,
    http: JobHttp,
}

#[derive(Deserialize)]
struct JobInfo {
    name: String,
}

#[derive(Deserialize)]
struct JobHttp {
    body: Option<JobBody>,
}

#[derive(Deserialize)]
struct JobBody {
    data: String,
}

#[derive(Deserialize)]
struct ResponseFile {
    runtime: RuntimeBlock,
}

#[derive(Deserialize)]
struct RuntimeBlock {
    scripts: Vec<ScriptBlock>,
}

#[derive(Deserialize)]
struct ScriptBlock {
    #[serde(rename = "type")]
    kind: String,
    code: String,
}

#[derive(Deserialize)]
struct HeldOutCase {
    text: String,
    expected: String,
}

/// Jalankan sweep. `Ok(true)` berarti nol mismatch.
pub async fn run(foundation: &Foundation) -> anyhow::Result<bool> {
    let (catalog, catalog_version_id) = chat::catalog::prepare(foundation).await?;
    let pool = foundation.app_db().pool();

    let mut cases = manifest_example_cases(&catalog);
    let manifest_count = cases.len();

    let answer_cases = answer_job_cases(&catalog)?;
    let answer_count = answer_cases.len();
    cases.extend(answer_cases);

    let selection_cases = retrieval_selection_cases()?;
    let selection_count = selection_cases.len();
    cases.extend(selection_cases);

    let held_out = held_out_cases()?;

    println!(
        "korpus: {} contoh manifest, {} pasangan answers/, {} pasangan retrieval-selection/, {} held-out (FIN-142)",
        manifest_count,
        answer_count,
        selection_count,
        held_out.len()
    );
    println!();

    let corpus_mismatches = evaluate(pool, catalog_version_id, &cases, "corpus").await?;
    println!();
    let held_out_cases: Vec<CorpusCase> = held_out
        .into_iter()
        .map(|case| CorpusCase {
            text: case.text,
            expected: case.expected,
            source: "held_out".to_string(),
        })
        .collect();
    let held_out_mismatches =
        evaluate(pool, catalog_version_id, &held_out_cases, "held-out").await?;

    println!();
    println!(
        "Ringkasan: corpus {}/{} cocok, held-out {}/{} cocok",
        cases.len() - corpus_mismatches,
        cases.len(),
        held_out_cases.len() - held_out_mismatches,
        held_out_cases.len()
    );

    Ok(corpus_mismatches == 0 && held_out_mismatches == 0)
}

async fn evaluate(
    pool: &sqlx::PgPool,
    catalog_version_id: uuid::Uuid,
    cases: &[CorpusCase],
    label: &str,
) -> anyhow::Result<usize> {
    let mut mismatches = 0usize;
    for case in cases {
        let candidate = planner::lexical_candidate(pool, catalog_version_id, &case.text).await?;
        let got = candidate.as_ref().map(|(id, _)| id.as_str());
        if got != Some(case.expected.as_str()) {
            mismatches += 1;
            println!(
                "MISMATCH [{}/{}] \"{}\" — expected {}, got {}",
                label,
                case.source,
                case.text,
                case.expected,
                got.unwrap_or("<none>")
            );
        }
    }
    println!("{label}: {mismatches} mismatch dari {} kasus", cases.len());
    Ok(mismatches)
}

fn manifest_example_cases(catalog: &Catalog) -> Vec<CorpusCase> {
    catalog
        .capabilities
        .iter()
        .flat_map(|loaded| {
            let capability_id = loaded.entry.id.clone();
            loaded.entry.examples.iter().map(move |example| CorpusCase {
                text: example.clone(),
                expected: capability_id.clone(),
                source: "manifest_example".to_string(),
            })
        })
        .collect()
}

/// `fineract-assistant-api/answers/**/*-job.yml`: `info.name` selalu
/// "<capability_id> job" (konvensi FIN-52) — pasangan yang capability id-nya
/// tidak ada di katalog termuat dilewati dengan peringatan, bukan gagal diam.
fn answer_job_cases(catalog: &Catalog) -> anyhow::Result<Vec<CorpusCase>> {
    let known: std::collections::BTreeSet<&str> = catalog
        .capabilities
        .iter()
        .map(|loaded| loaded.entry.id.as_str())
        .collect();

    let mut cases = Vec::new();
    for path in find_files(Path::new("fineract-assistant-api/answers"), "-job.yml") {
        let raw = fs::read_to_string(&path)?;
        let job: JobFile = serde_yaml::from_str(&raw)?;
        let Some(capability_id) = job.info.name.strip_suffix(" job") else {
            continue;
        };
        if !known.contains(capability_id) {
            println!(
                "peringatan: {} menamai capability '{}' yang tidak ada di katalog termuat — dilewati",
                path.display(),
                capability_id
            );
            continue;
        }
        let Some(body) = job.http.body else { continue };
        let payload: serde_json::Value = serde_json::from_str(&body.data)?;
        let Some(text) = payload.get("request_text").and_then(|v| v.as_str()) else {
            continue;
        };
        cases.push(CorpusCase {
            text: text.to_string(),
            expected: capability_id.to_string(),
            source: path.display().to_string(),
        });
    }
    Ok(cases)
}

/// `fineract-assistant-api/retrieval-selection/*-response.yml`: assertion
/// `lineage[0].capability_id).to.equal("<id>")` dipasangkan dengan
/// `request_text` dari `*-job.yml` bernama sama.
fn retrieval_selection_cases() -> anyhow::Result<Vec<CorpusCase>> {
    let mut cases = Vec::new();
    for path in find_files(
        Path::new("fineract-assistant-api/retrieval-selection"),
        "-response.yml",
    ) {
        let raw = fs::read_to_string(&path)?;
        let response: ResponseFile = serde_yaml::from_str(&raw)?;
        let Some(expected) = response
            .runtime
            .scripts
            .iter()
            .find(|script| script.kind == "tests")
            .and_then(|script| extract_expected_capability(&script.code))
        else {
            continue;
        };

        let job_path = path.to_string_lossy().replace("-response.yml", "-job.yml");
        let job_raw = fs::read_to_string(&job_path)?;
        let job: JobFile = serde_yaml::from_str(&job_raw)?;
        let Some(body) = job.http.body else { continue };
        let payload: serde_json::Value = serde_json::from_str(&body.data)?;
        let Some(text) = payload.get("request_text").and_then(|v| v.as_str()) else {
            continue;
        };

        cases.push(CorpusCase {
            text: text.to_string(),
            expected,
            source: path.display().to_string(),
        });
    }
    Ok(cases)
}

/// Cari `lineage[0].capability_id).to.equal("<id>")` di sumber test JS.
fn extract_expected_capability(code: &str) -> Option<String> {
    let marker = "capability_id).to.equal(\"";
    let start = code.find(marker)? + marker.len();
    let end = code[start..].find('"')?;
    Some(code[start..start + end].to_string())
}

fn held_out_cases() -> anyhow::Result<Vec<HeldOutCase>> {
    let raw = fs::read_to_string("crates/app/fixtures/fin142_held_out.json")?;
    Ok(serde_json::from_str(&raw)?)
}

fn find_files(root: &Path, suffix: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.to_string_lossy().ends_with(suffix) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
