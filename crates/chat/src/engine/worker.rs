//! Worker: mengklaim job, menjaga lease, merencanakan, mengeksekusi, lalu
//! menyelesaikannya.
//!
//! Satu Engine memiliki orchestration end-to-end (PRD §4). Alurnya:
//! klaim (T2) → scope terotorisasi → plan (T3) → eksekusi node di luar
//! transaksi (T4) → komposisi deterministik → commit response (T7).
//!
//! Permintaan yang tidak tercakup capability yang disetujui **tidak dijawab**:
//! hasilnya `Unsupported` dengan response `kind='limitation'` yang menyebut
//! sebabnya (PRD §5). Itu bukan kegagalan operasional, dan bukan pula jawaban
//! kosong yang berpura-pura menjawab.

use std::{sync::Arc, time::Duration};

use foundation::embedding::EmbeddingClient;
use foundation::state::Foundation;
use sha2::{Digest, Sha256};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    catalog::Catalog,
    clarification::repository::{self as clarification_repository, AcceptedAnswer},
    engine::{
        compose, dataset, executor, memory,
        planner::{self, Plan},
        repository::{self, ClaimedJob, NodeOutcome, SettledResponse},
        resolver,
        validate::{self, Validated},
        write_intent,
    },
};

/// Satu-satunya versi plan yang dihasilkan planner deterministik. Re-plan baru
/// relevan ketika klarifikasi atau invalidasi output ada.
const PLAN_VERSION: i32 = 1;

/// Jalankan loop worker sampai `shutdown` dibatalkan.
pub async fn run(
    foundation: Foundation,
    catalog: Arc<Catalog>,
    catalog_version_id: Uuid,
    shutdown: CancellationToken,
) {
    let config = foundation.config();
    let embedding = match EmbeddingClient::new(config) {
        Ok(client) => Arc::new(client),
        Err(error) => {
            error!(%error, "klien embedding gagal dibuat; worker berhenti");
            return;
        }
    };
    let worker = worker_identity();
    let poll = Duration::from_millis(config.worker_poll_interval_ms);
    let mut crash = CrashSeam {
        remaining: config.local_crash_after_external_call.unwrap_or(0),
    };

    info!(%worker, "worker berjalan");

    loop {
        if shutdown.is_cancelled() {
            break;
        }

        match repository::claim_next(
            foundation.app_db().pool(),
            &worker,
            config.worker_lease_duration_secs,
            config.job_ttl_running_secs,
        )
        .await
        {
            Ok(Some(job)) => {
                if let Err(error) = process(
                    &foundation,
                    &catalog,
                    catalog_version_id,
                    &embedding,
                    &worker,
                    job,
                    &mut crash,
                )
                .await
                {
                    error!(error = %error, "job gagal diproses");
                }
                // Langsung lanjut: mungkin masih ada antrean.
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                // Database bermasalah; jangan memutar loop sekencang mungkin.
                error!(error = %error, "klaim job gagal");
                sleep(poll).await;
                continue;
            }
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = sleep(poll) => {}
        }
    }

    info!(%worker, "worker berhenti");
}

/// Seam `LOCAL_CRASH_AFTER_EXTERNAL_CALL` (FIN-56; config menolaknya di luar
/// `APP_ENV=local`). `remaining` = berapa panggilan sumber lagi yang
/// ditinggalkan; nol di luar test, sehingga [`CrashSeam::fire`] tidak pernah
/// benar.
struct CrashSeam {
    remaining: u32,
}

impl CrashSeam {
    fn fire(&mut self) -> bool {
        let fire = self.remaining > 0;
        self.remaining = self.remaining.saturating_sub(1);
        fire
    }
}

async fn process(
    foundation: &Foundation,
    catalog: &Catalog,
    catalog_version_id: Uuid,
    embedding: &EmbeddingClient,
    worker: &str,
    job: ClaimedJob,
    crash: &mut CrashSeam,
) -> anyhow::Result<()> {
    let pool = foundation.app_db().pool().clone();
    let config = foundation.config();

    // K1: heartbeat WAJIB dari task independen. Bila ia hanya dipancarkan di
    // batas node, satu node lambat membuat worker sehat dipagari di tengah kerja.
    let fenced = CancellationToken::new();
    let heartbeat = tokio::spawn(heartbeat_loop(
        pool.clone(),
        job.id,
        job.lease_token,
        config.worker_lease_duration_secs,
        Duration::from_secs(config.worker_lease_heartbeat_interval_secs),
        fenced.clone(),
    ));

    let settled = if repository::cancel_requested(&pool, job.id).await? {
        repository::settle_cancelled(&pool, job.id, job.session_id, job.lease_token).await?
    } else {
        run_job(
            foundation,
            catalog,
            catalog_version_id,
            embedding,
            &job,
            crash,
        )
        .await?
    };

    fenced.cancel();
    let _ = heartbeat.await;

    if settled {
        info!(job_id = %job.id, %worker, "job diselesaikan");
    } else {
        // Fencing kalah: worker lain sudah memegang job ini, atau reaper sudah
        // menutupnya. Berhenti tanpa mencoba transisi alternatif (engine.md §5).
        warn!(job_id = %job.id, %worker, "worker dipagari; tidak ada yang ditulis");
    }

    Ok(())
}

async fn heartbeat_loop(
    pool: sqlx::PgPool,
    job_id: Uuid,
    lease_token: Uuid,
    lease_duration_secs: i64,
    interval: Duration,
    fenced: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = fenced.cancelled() => break,
            _ = sleep(interval) => {}
        }

        match repository::renew_lease(&pool, job_id, lease_token, lease_duration_secs).await {
            Ok(true) => {}
            Ok(false) => {
                warn!(%job_id, "lease tidak dapat diperpanjang; worker dipagari");
                fenced.cancel();
                break;
            }
            Err(error) => {
                error!(%job_id, error = %error, "renewal lease gagal");
            }
        }
    }
}

/// Jalankan satu job: plan → eksekusi → komposisi → commit.
async fn run_job(
    foundation: &Foundation,
    catalog: &Catalog,
    catalog_version_id: Uuid,
    embedding: &EmbeddingClient,
    job: &ClaimedJob,
    crash: &mut CrashSeam,
) -> anyhow::Result<bool> {
    let pool = foundation.app_db().pool();

    // OVR-6.6 — Jarvis read-only terhadap Fineract. Perintah mengubah data
    // ditolak sebelum scope, retrieval, plan, dan query sumber apa pun; ia
    // tidak boleh jatuh ke capability baca terdekat lalu dijawab (FIN-139).
    if write_intent::is_write_request(&job.request_text) {
        return settle_blocked(
            foundation,
            job,
            WRITE_NOT_SUPPORTED,
            "Jarvis is read-only: it never creates, changes or deletes data in Fineract, so this request was not run.",
        )
        .await;
    }

    // OVR-6.6 — permintaan atas permukaan yang tidak disetujui (field rahasia,
    // tabel di luar cakupan, intent yang dinyatakan tidak didukung domain)
    // ditolak di sini, sebelum retrieval sempat memetakannya ke capability
    // baca terdekat (unsupported_requests.yaml `hard_reject_maps_to_policy`).
    // Kosakatanya dari knowledge; lihat `catalog::surface` (FIN-139).
    if let Some(term) = catalog.unapproved_surfaces.find(&job.request_text) {
        warn!(job_id = %job.id, source = term.source, term = %term.name, "permukaan tidak disetujui diminta; ditolak");
        return settle_blocked(
            foundation,
            job,
            SURFACE_NOT_APPROVED,
            "This request asks for data outside what Jarvis is approved to report on, so no query was run.",
        )
        .await;
    }

    // Scope dari otorisasi, dipersempit oleh permintaan — tidak pernah
    // diperlebar olehnya (I7).
    let requested_offices = requested_office_ids(&job.scope_json);
    let authorized = match executor::authorized_office_ids(
        foundation.fineract_db(),
        &requested_offices,
    )
    .await
    {
        Ok(offices) => offices,
        Err(error) => {
            warn!(job_id = %job.id, code = error.failure_code(), "scope tidak dapat diturunkan");
            return settle_operational_failure(foundation, job, error.failure_code()).await;
        }
    };

    // OVR-6.6 / I7 — office yang diminta tetapi tidak diizinkan adalah upaya
    // MEMPERLEBAR scope. Ia ditolak eksplisit sebelum plan dan sebelum query
    // sumber mana pun, bukan dibuang diam-diam lalu dijawab atas sisanya.
    let widened = unauthorized_offices(&requested_offices, &authorized);
    if !widened.is_empty() {
        warn!(job_id = %job.id, offices = ?widened, "office di luar otorisasi diminta; ditolak");
        return settle_blocked(
            foundation,
            job,
            OFFICE_SCOPE_NOT_AUTHORIZED,
            "The requested office scope includes offices you are not authorized to read, so Jarvis did not run any query.",
        )
        .await;
    }

    // Jawaban klarifikasi yang sudah diterima dibaca lebih dulu: slot yang
    // sudah dijawab tidak pernah ditanyakan ulang (clarifications.md).
    let supplied = clarification_repository::accepted_answers(pool, job.id).await?;

    let planned = planner::plan(
        pool,
        embedding,
        foundation.config().embedding_similarity_cutoff,
        catalog,
        catalog_version_id,
        &job.request_text,
        &authorized,
        &supplied,
    )
    .await?;

    let plan = match planned {
        Ok(plan) => plan,
        // Parameter yang kurang dan dapat ditanyakan → tanyakan (T5), jangan
        // tolak. Job yang sama ditangguhkan; tidak ada job pengganti. Slot
        // identitas tanpa resolver TIDAK dapat ditanyakan (K1) dan jatuh ke arm
        // berikutnya sebagai `Unsupported`.
        Err(planner::Unplannable::NeedsClarification {
            capability,
            missing,
        }) if !missing.iter().any(planner::Missing::unanswerable) => {
            return open_clarification(foundation, job, &capability, &missing, &authorized).await;
        }
        Err(problem) => {
            return repository::settle_with_response(
                pool,
                job.id,
                job.session_id,
                job.owner_user_id,
                job.lease_token,
                Validated::unchecked(limitation_response(
                    &problem.reason(),
                    &problem.explain(),
                    &job.request_text,
                )),
                // Pertanyaan yang tidak dijawab tidak meninggalkan fakta: tidak
                // ada hasil, dan tidak ada scope yang menghasilkan apa pun.
                &[],
            )
            .await
            .map_err(Into::into);
        }
    };

    let contract_versions = serde_json::json!({
        "catalog_version_id": plan.catalog_version_id,
        "catalog_content_hash": plan.catalog_content_hash,
        "capability_id": plan.capability_id,
        "query_id": plan.query_id,
    });

    let persisted = repository::persist_plan(
        pool,
        job.id,
        job.session_id,
        job.lease_token,
        PLAN_VERSION,
        &plan.graph_json,
        &plan.graph_hash,
        &contract_versions,
        &plan.capability_id,
    )
    .await?;

    match persisted {
        repository::PlanPersisted::Persisted | repository::PlanPersisted::Adopted => {}
        repository::PlanPersisted::Fenced => return Ok(false),
        // D4 — plan versi ini milik attempt sebelumnya; menjalankan node di
        // bawahnya dengan graph/katalog lain berarti ledger berbohong tentang
        // apa yang dieksekusi. Re-plan (`plan_version` baru) belum ada.
        repository::PlanPersisted::Changed => {
            warn!(job_id = %job.id, "plan berubah saat recovery; node tidak dijalankan");
            return settle_failed_with(
                foundation,
                job,
                PLAN_CHANGED_ON_RECOVERY,
                "The analysis plan changed while this request was being recovered, so the \
                 approved source query was not run again and no figure is reported.",
            )
            .await;
        }
    }

    // Admisi TEPAT sebelum query dikirim: sejak baris ini `Running`, lease
    // yang hilang berarti outcome-nya tidak pasti (I4), dan reaper
    // menandainya `Abandoned`, bukan `Failed`.
    let attempt = match repository::admit_node(pool, job.id, job.lease_token, PLAN_VERSION).await? {
        repository::Admission::Admitted(attempt) => attempt,
        repository::Admission::Fenced => return Ok(false),
        // Lease hilang sesudah T4 dan sebelum T7: output node sudah durable.
        // Output `Completed` tidak dijalankan ulang (engine.md), dan reuse
        // untuk menyusun response darinya belum ada — tutup eksplisit,
        // jangan kembalikan ke antrean tanpa ujung.
        repository::Admission::NothingRunnable => {
            warn!(job_id = %job.id, "node sudah Completed; tidak dijalankan ulang");
            return settle_failed_with(
                foundation,
                job,
                COMPLETED_NODE_NOT_RERUN,
                "The approved source query completed, but the worker was lost before the \
                     answer was committed. A completed query is not run again, and building \
                     the answer from its stored result is not supported yet, so no figure is \
                     reported.",
            )
            .await;
        }
    };

    // Di luar transaksi mana pun (I1).
    let executed = executor::execute(foundation.fineract_db(), &plan).await;

    // OVR-6.4 — query sudah kembali, T4 belum commit. Seam lokal meninggalkan
    // attempt di titik ini persis seperti worker yang mati: heartbeat berhenti
    // (lihat `process`) dan tidak ada yang ditulis.
    if crash.fire() {
        warn!(
            job_id = %job.id,
            attempt,
            "LOCAL_CRASH_AFTER_EXTERNAL_CALL: attempt ditinggalkan sesudah query, sebelum T4"
        );
        return Ok(false);
    }

    match executed {
        Ok(mut result) => {
            // FIN-133 — executor mengikat `cap + 1`; kelebihan baris dipotong
            // di sini, SEBELUM apa pun disimpan: ledger, dataset, response, dan
            // memori melihat set yang sama, dan pemotongannya dinyatakan.
            let row_cap = compose::cap_rows(&plan, &mut result.rows);

            // Sakelar PII disnapshot saat job diterima (#15), bukan dibaca ulang
            // sekarang: laporan tidak boleh berubah makna karena konfigurasi
            // berubah di tengah eksekusi.
            let pii_enabled = pii_enabled(job);

            // K5 — dibaca dari yang tersimpan, bukan dari ingatan proses ini:
            // job yang dilanjutkan worker lain tetap mengungkap auto-bind-nya.
            let auto_bound = clarification_repository::auto_bound_slots(pool, job.id).await?;

            let (visible, withheld) = compose::visible_fields(&plan, pii_enabled);

            // Kolom yang ditahan dibuang SEBELUM disimpan — sekali, lalu dipakai
            // baik oleh ledger maupun oleh chunk dataset: dua jalur penyimpanan
            // dengan redaksi masing-masing adalah dua tempat PII dapat bocor.
            let stored_rows = compose::redact(&result.rows, &withheld);

            // Node dipersist LEBIH DULU: `derived_from` merujuk identitas baris
            // ledger (responses.md §1), dan identitas yang belum durable bukan
            // identitas. Komposisi karena itu menunggu hasil tulisan ini.
            let node_run_id = repository::complete_node(
                pool,
                job.id,
                job.session_id,
                job.lease_token,
                PLAN_VERSION,
                attempt,
                NodeOutcome {
                    status: "Completed",
                    completeness: Some(if row_cap.is_some() {
                        "Partial"
                    } else {
                        "Complete"
                    }),
                    completeness_reason: row_cap.map(|_| compose::ROW_CAP_REACHED),
                    failure_code: None,
                    // Hasil kecil disimpan inline; dataset berchunk baru
                    // diperlukan saat hasil besar, dan belum ada konsumennya.
                    // Kolom yang ditahan dibuang SEBELUM disimpan: PII yang
                    // hanya disembunyikan dari response tetap tersimpan, dan
                    // yang tersimpan cepat atau lambat terbaca.
                    output_json: Some(serde_json::json!({
                        "rows": stored_rows,
                        "withheld_columns": withheld,
                    })),
                    // Ledger D2: binding yang benar-benar dikonsumsi node,
                    // termasuk slot yang diikat tanpa bertanya. Validator
                    // membacanya kembali dari sini, bukan dari `auto_bound` di
                    // atas — itu yang membuat pemeriksaannya bukan cermin.
                    input_binding_json: serde_json::json!({
                        "parameters": compose::bindings(&plan),
                        "auto_bound_slots": auto_bound
                            .iter()
                            .map(|slot| slot.field_id.as_str())
                            .collect::<Vec<_>>(),
                    }),
                    provenance_json: node_provenance(&plan, result.rows.len()),
                    rows_returned: Some(result.rows.len() as i64),
                    duration_ms: Some(result.duration_ms),
                },
            )
            .await?;

            let Some(node_run_id) = node_run_id else {
                return Ok(false);
            };

            // L3 — baris hasil diretensi sebagai handle + chunk, bukan hanya
            // dibuang inline ke ledger: tabel berpaginasi, node hilir, dan
            // memori session merujuk HANDLE, bukan daftar baris yang dibentangkan
            // (#11 aturan 2 dan 3). Handle-nya immutable begitu `ready`.
            let Some(retained) = retain_dataset(
                foundation,
                job,
                &plan,
                node_run_id,
                &visible,
                &withheld,
                &authorized,
                &stored_rows,
                row_cap,
            )
            .await?
            else {
                // Fencing kalah saat meretensi: berhenti, jangan menulis response
                // atas snapshot yang tidak jadi ada (C16).
                return Ok(false);
            };

            let response = compose::analysis(
                &plan,
                &result.rows,
                result.duration_ms,
                pii_enabled,
                &auto_bound,
                node_run_id,
                &retained,
                row_cap,
            );

            // D1–D3 sesudah ledger durable, bukan sebelum: yang divalidasi
            // adalah kecocokan dokumen dengan apa yang tersimpan.
            let ledger = repository::ledger(pool, job.id, PLAN_VERSION).await?;
            let validated = validate::apply(response, &ledger);

            if validated.status() != "passed" {
                warn!(
                    job_id = %job.id,
                    report = %validated.report,
                    "response ditolak validator; menyajikan fallback deterministik"
                );
            }

            let response = &validated.served;

            // Identitas dibaca dari yang tersimpan, sama seperti K5 di atas:
            // job yang dilanjutkan worker lain tetap mempromosikan identitas
            // yang benar-benar terikat, dengan provenance aslinya.
            let identities = clarification_repository::answered_identities(pool, job.id).await?;
            // Fakta diturunkan dari dokumen yang DISAJIKAN: memori tidak boleh
            // membawa klaim yang baru saja ditolak ke turn berikutnya.
            let facts = memory::promoted(&plan, response, &identities, result.rows.len());

            repository::settle_with_response(
                pool,
                job.id,
                job.session_id,
                job.owner_user_id,
                job.lease_token,
                validated,
                &facts,
            )
            .await
            .map_err(Into::into)
        }
        Err(error) => {
            let failure_code = error.failure_code();
            warn!(job_id = %job.id, code = failure_code, "eksekusi query gagal");

            // Kegagalan node tetap dicatat di ledger: tanpa ini, investigasi
            // hanya melihat job gagal tanpa tahu operasi mana yang gagal.
            repository::complete_node(
                pool,
                job.id,
                job.session_id,
                job.lease_token,
                PLAN_VERSION,
                attempt,
                NodeOutcome {
                    status: "Failed",
                    // Hasilnya TIDAK DIKETAHUI, bukan nol (I4).
                    completeness: Some("Unknown"),
                    completeness_reason: None,
                    failure_code: Some(failure_code),
                    // Binding tetap dicatat meski node gagal: "dengan parameter
                    // apa ia gagal" adalah separuh dari investigasinya.
                    input_binding_json: serde_json::json!({
                        "parameters": compose::bindings(&plan),
                        "auto_bound_slots": [],
                    }),
                    output_json: None,
                    provenance_json: node_provenance(&plan, 0),
                    rows_returned: None,
                    duration_ms: None,
                },
            )
            .await?;

            settle_operational_failure(foundation, job, failure_code).await
        }
    }
}

/// Retensi hasil node sebagai dataset (§3): chunk ditulis, handle menjadi
/// `ready`, lalu ledger node menunjuknya (C5).
///
/// `None` berarti fencing kalah dan tidak ada apa pun yang ditulis; `Some`
/// membawa handle yang sudah `ready` dan tertaut ke node run — satu-satunya id
/// yang boleh muncul di lineage response (FIN-43) — beserta pemotongannya,
/// yang wajib dinyatakan response (DS-8.1).
///
/// ponytail: setiap hasil yang berhasil diretensi — bukan hanya yang besar.
/// #11 aturan 3 mengizinkan hasil kecil hidup inline saja, tetapi setiap
/// response hari ini memuat blok `table`, dan blok tabel adalah salah satu
/// alasan retensi yang disebut aturan itu. Ambang "cukup besar" menunggu
/// ukuran data nyata (runtime.md §4 masih menunggu uji kapasitas); menebak
/// angkanya sekarang hanya memindahkan keputusan ke tempat yang lebih sulit
/// dilihat.
#[allow(clippy::too_many_arguments)]
async fn retain_dataset(
    foundation: &Foundation,
    job: &ClaimedJob,
    plan: &Plan,
    node_run_id: Uuid,
    visible: &[String],
    withheld: &[String],
    authorized: &[i64],
    rows: &[serde_json::Map<String, serde_json::Value>],
    row_cap: Option<compose::RowCapReached>,
) -> anyhow::Result<Option<dataset::Retained>> {
    let pool = foundation.app_db().pool();
    let config = foundation.config();
    let materialized = dataset::materialize(rows, dataset::row_cap(config.local_dataset_max_rows));

    let claim = handle_claim(
        materialized.truncation,
        materialized.row_count_total,
        row_cap,
    );
    debug_assert!(dataset::claim_is_stated(
        materialized.truncated(),
        claim.completeness,
        claim.completeness_reason
    ));
    let truncation = materialized.truncation.map(|reason| dataset::Truncation {
        reason,
        row_count_available: materialized.row_count_available,
    });

    let mut provenance = node_provenance(plan, rows.len());
    // Snapshot menyatakan titik datanya; kesegaran SUMBER terpisah dari ini (§3).
    provenance["as_of"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());

    let created = dataset::repository::create(
        pool,
        dataset::repository::NewDataset {
            job_id: job.id,
            session_id: job.session_id,
            owner_user_id: job.owner_user_id,
            lease_token: job.lease_token,
            node_id: "main",
            plan_version: PLAN_VERSION,
            schema_json: serde_json::json!({
                "fields": visible,
                "withheld_columns": withheld,
            }),
            // Grain belum dideklarasikan planner; kosong berarti belum
            // dinyatakan, dan itu tidak boleh dibaca sebagai "tanpa fanout".
            grain_json: serde_json::json!({}),
            // Scope yang BENAR-BENAR dipakai mengeksekusi, bukan yang diminta.
            scope_json: serde_json::json!({ "office_ids": authorized }),
            provenance_json: provenance,
            // Urutan dibekukan pada saat materialisasi: chunk menyimpan baris
            // persis seperti yang dikembalikan query yang disetujui, dan
            // paginasi mengikuti ordinal itu.
            sort_key_json: serde_json::json!(["__row_ordinal"]),
            completeness: claim.completeness,
            completeness_reason: claim.completeness_reason,
            truncated: materialized.truncated(),
            row_count_available: materialized.row_count_available,
            row_count_total: claim.row_count_total,
            byte_size: materialized.byte_size,
            ttl_secs: dataset::ttl_secs(
                config.clarification_wait_limit_secs,
                config.job_ttl_running_secs,
            ),
            chunks: materialized.chunks,
        },
    )
    .await?;

    let Some(dataset_id) = created else {
        return Ok(None);
    };

    let linked =
        dataset::repository::link_node_run(pool, node_run_id, dataset_id, job.id, job.lease_token)
            .await?;
    Ok(linked.then_some(dataset::Retained {
        dataset_id,
        truncation,
    }))
}

/// Klaim analitik atas satu handle dataset (dataset-lifecycle §5, I4).
#[derive(Debug, PartialEq, Eq)]
struct HandleClaim {
    completeness: &'static str,
    completeness_reason: Option<&'static str>,
    row_count_total: Option<i64>,
}

/// Dua batas yang berbeda, satu klaim.
///
/// - Cap SIMPAN (`dataset_row_cap_reached`/`dataset_byte_cap_reached`):
///   `truncated=true`, handle menyimpan lebih sedikit daripada yang dilihat
///   node. Jawaban response tetap dihitung atas seluruh baris node.
/// - Row cap capability (`row_cap_reached`, FIN-133): node sendiri hanya
///   melihat `cap` baris dari populasi yang lebih besar. Set yang tersimpan
///   tidak terpotong (`truncated` tetap dari cap simpan), tetapi klaim
///   analitiknya `Partial` dan total populasinya **tidak diketahui** —
///   `row_count_total = NULL`, bukan `cap` (I4): menulis `cap` akan membuat
///   handle tampak memuat seluruh populasi.
///
/// Bila keduanya tercapai, alasan cap simpan yang ditulis: api-reference
/// menjanjikan handle terpotong menyebut cap simpannya.
fn handle_claim(
    storage_truncation: Option<&'static str>,
    row_count_total: Option<i64>,
    row_cap: Option<compose::RowCapReached>,
) -> HandleClaim {
    let completeness_reason = storage_truncation.or(row_cap.map(|_| compose::ROW_CAP_REACHED));

    HandleClaim {
        completeness: if completeness_reason.is_some() {
            "Partial"
        } else {
            "Complete"
        },
        completeness_reason,
        row_count_total: if row_cap.is_some() {
            None
        } else {
            row_count_total
        },
    }
}

/// T5 — tangguhkan job dan terbitkan satu form berisi seluruh slot yang kurang.
///
/// Slot ber-resolver dijalankan lebih dulu, karena hasilnya menentukan bentuk
/// pertanyaannya: nol kandidat berarti tidak ada yang dapat ditanyakan, satu
/// kandidat diikat tanpa bertanya (K5), lebih dari satu menjadi `single_choice`.
async fn open_clarification(
    foundation: &Foundation,
    job: &ClaimedJob,
    capability: &str,
    missing: &[planner::Missing],
    authorized: &[i64],
) -> anyhow::Result<bool> {
    let pii_enabled = pii_enabled(job);
    let mut fields: Vec<serde_json::Value> = Vec::with_capacity(missing.len());
    let mut auto_bound: Vec<AcceptedAnswer> = Vec::new();

    for item in missing {
        let mut field = serde_json::json!({
            "field_id": item.name,
            "type": item.field_type(),
            // Tipe parameter ikut dibawa supaya validasi jawaban memakai
            // kontrak yang sama dengan pengikatan parameter — bukan dua
            // aturan yang dapat menyimpang.
            "parameter_kind": item.kind,
            "label": item.name.replace('_', " "),
            "required": true,
        });

        let Some(slot) = &item.resolver else {
            fields.push(field);
            continue;
        };

        let candidates = match resolver::candidates(
            foundation.fineract_db(),
            slot,
            authorized,
            pii_enabled,
            None,
            foundation.config().resolver_max_candidates,
        )
        .await
        {
            Ok(candidates) => candidates,
            Err(error) => {
                warn!(job_id = %job.id, code = error.failure_code(), slot = %item.name, "resolver gagal");
                return settle_operational_failure(foundation, job, error.failure_code()).await;
            }
        };

        // Nol kandidat: tidak ada yang dapat dipilih, dan menanyakan slot yang
        // tidak punya jawaban sah hanya memindahkan kebuntuan ke pengguna.
        if candidates.items.is_empty() {
            return repository::settle_with_response(
                foundation.app_db().pool(),
                job.id,
                job.session_id,
                job.owner_user_id,
                job.lease_token,
                Validated::unchecked(not_found_response(
                    &item.name,
                    &slot.query_id,
                    &job.request_text,
                )),
                &[],
            )
            .await
            .map_err(Into::into);
        }

        if candidates.matched_total == 1 {
            let only = &candidates.items[0];
            auto_bound.push(AcceptedAnswer {
                field_id: item.name.clone(),
                answer_kind: "option_id",
                raw_text: None,
                binding_json: serde_json::json!({
                    "value": resolver::binding_text(&only.binding),
                    "label": only.label,
                }),
                // K5 — BUKAN `user_confirmed`: tidak ada yang mengonfirmasi.
                provenance: "resolver_unique",
                resolver_ref: Some(slot.query_id.clone()),
                option_set_ref: Some(only.option_id.clone()),
            });
        }

        field["resolver"] = serde_json::json!({
            "dataset_id": slot.dataset_id,
            "shape_id": slot.shape_id,
            "output_slot": slot.binding_field,
        });
        field["resolver_ref"] = serde_json::Value::String(slot.query_id.clone());
        field["candidate_count"] = serde_json::Value::from(candidates.matched_total);
        field["candidates_truncated"] = serde_json::Value::Bool(candidates.truncated);
        // Opsi TIDAK disematkan di form: ia diterbitkan per halaman oleh
        // endpoint, dan hanya halaman yang benar-benar dikirim yang dicatat.
        field["options_path"] = serde_json::Value::String(format!(
            "/chat/jobs/{}/clarification/options?field_id={}",
            job.id, item.name
        ));
        fields.push(field);
    }

    let form = clarification_repository::open_form(
        foundation.app_db().pool(),
        job.id,
        job.session_id,
        job.lease_token,
        None,
        &format!("Missing input for capability '{capability}'"),
        "Additional input needed",
        &serde_json::Value::Array(fields),
        &auto_bound,
        foundation.config().clarification_wait_limit_secs,
        foundation.config().job_ttl_running_secs,
    )
    .await?;

    match form {
        Some(form) => {
            info!(
                job_id = %job.id,
                clarification_id = %form.clarification_id,
                state = %form.state,
                auto_bound = auto_bound.len(),
                "klarifikasi dibuka"
            );
            Ok(true)
        }
        // Fencing kalah.
        None => Ok(false),
    }
}

/// Sakelar PII yang berlaku untuk job ini, disnapshot saat diterima (#15).
fn pii_enabled(job: &ClaimedJob) -> bool {
    job.scope_json
        .get("pii")
        .and_then(|pii| pii.get("enabled"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// Resolver berjalan utuh dalam scope dan tidak menemukan satu pun kandidat.
///
/// `NotFound` + `Complete`: pencariannya selesai, bukan terpotong — dan
/// engine.md melarang `NotFound` berpasangan dengan `Partial`.
fn not_found_response(slot: &str, resolver_ref: &str, request_text: &str) -> SettledResponse {
    let blocks = serde_json::json!([compose::block(
        "resolver_no_candidates",
        "limitation",
        &[],
        serde_json::json!({
            "title": "Nothing to choose from",
            "body": format!(
                "The approved resolver '{resolver_ref}' found no candidate for '{slot}' inside your \
                 authorized scope, so there is nothing to select and the request is not answered."
            ),
            "request_echo": request_text,
        }),
    )]);

    SettledResponse {
        kind: "limitation",
        outcome: "NotFound",
        completeness: "Complete",
        completeness_reason: "resolver_no_candidates".to_string(),
        response_hash: hash_blocks(&blocks),
        // Tidak ada operasi sumber yang berjalan: tidak ada lineage untuk
        // dicatat, dan `{}` di sini berarti "tidak ada", bukan "belum diisi".
        evidence: serde_json::json!({}),
        blocks,
    }
}

async fn settle_operational_failure(
    foundation: &Foundation,
    job: &ClaimedJob,
    failure_code: &str,
) -> anyhow::Result<bool> {
    settle_failed_with(
        foundation,
        job,
        failure_code,
        "The approved source query did not complete, so no figure is reported. \
         The outcome of the attempt is unknown, not zero.",
    )
    .await
}

async fn settle_failed_with(
    foundation: &Foundation,
    job: &ClaimedJob,
    failure_code: &str,
    body: &str,
) -> anyhow::Result<bool> {
    repository::settle_failed(
        foundation.app_db().pool(),
        job.id,
        job.session_id,
        job.lease_token,
        failure_code,
        operational_failure_response(failure_code, body),
    )
    .await
    .map_err(Into::into)
}

/// Response yang disajikan reaper saat attempt tak pasti mencapai
/// `NODE_ATTEMPT_CAP` (OVR-6.4, engine.md langkah 5): menyatakan bahwa query
/// mungkin sudah berjalan, bukan bahwa ia gagal.
pub fn attempts_exhausted_response(node_attempt_cap: i32) -> SettledResponse {
    operational_failure_response(
        repository::NODE_ATTEMPT_CAP_REACHED,
        &format!(
            "The approved source query was attempted {node_attempt_cap} times and the \
             outcome of every attempt is unknown, so no figure is reported and it is not \
             retried again. Unknown is not zero."
        ),
    )
}

/// `Failed` + `OperationalFailure` + `Unknown` (engine.md): tidak ada klaim
/// atas data sumber, dan sebabnya dinyatakan pada satu blok `limitation`.
fn operational_failure_response(failure_code: &str, body: &str) -> SettledResponse {
    let blocks = serde_json::json!([compose::block(
        failure_code,
        "limitation",
        &[],
        serde_json::json!({
            "title": "Request not answered",
            "body": body,
        }),
    )]);

    SettledResponse {
        kind: "limitation",
        outcome: "OperationalFailure",
        completeness: "Unknown",
        completeness_reason: failure_code.to_string(),
        response_hash: hash_blocks(&blocks),
        evidence: serde_json::json!({}),
        blocks,
    }
}

/// Penyempitan office yang diminta saat job diterima (snapshot `scope_json`).
fn requested_office_ids(scope_json: &serde_json::Value) -> Vec<i64> {
    scope_json
        .get("office_ids")
        .and_then(|value| value.as_array())
        .map(|ids| ids.iter().filter_map(serde_json::Value::as_i64).collect())
        .unwrap_or_default()
}

/// Alasan penolakan scope yang melebar (OVR-6.6).
const OFFICE_SCOPE_NOT_AUTHORIZED: &str = "office_scope_not_authorized";
/// Alasan penolakan perintah tulis (OVR-6.6, FIN-139).
const WRITE_NOT_SUPPORTED: &str = "write_not_supported";
/// Alasan penolakan permukaan yang tidak disetujui (OVR-6.6, FIN-139).
const SURFACE_NOT_APPROVED: &str = "surface_not_approved";
/// Plan versi aktif yang diverifikasi ulang saat recovery tidak lagi identik
/// dengan yang tersimpan (D4); re-plan belum ada, jadi job tidak dijawab.
const PLAN_CHANGED_ON_RECOVERY: &str = "plan_changed_on_recovery";
/// Lease hilang sesudah node `Completed` tetapi sebelum response commit.
/// Output itu tidak dijalankan ulang, dan reuse-nya belum ada (engine.md).
const COMPLETED_NODE_NOT_RERUN: &str = "completed_node_not_rerun";

/// Tutup job dengan penolakan kebijakan: tanpa plan, tanpa node, tanpa query
/// sumber, tanpa fakta memori.
async fn settle_blocked(
    foundation: &Foundation,
    job: &ClaimedJob,
    reason: &str,
    explanation: &str,
) -> anyhow::Result<bool> {
    repository::settle_with_response(
        foundation.app_db().pool(),
        job.id,
        job.session_id,
        job.owner_user_id,
        job.lease_token,
        Validated::unchecked(blocked_response(reason, explanation, &job.request_text)),
        &[],
    )
    .await
    .map_err(Into::into)
}

/// Office yang diminta tetapi tidak ada di otorisasi pemanggil.
fn unauthorized_offices(requested: &[i64], authorized: &[i64]) -> Vec<i64> {
    requested
        .iter()
        .copied()
        .filter(|office| !authorized.contains(office))
        .collect()
}

fn node_provenance(plan: &Plan, row_count: usize) -> serde_json::Value {
    serde_json::json!({
        "capability_id": plan.capability_id,
        "query_id": plan.query_id,
        "sql_file": plan.sql_file,
        "catalog_version_id": plan.catalog_version_id,
        "catalog_content_hash": plan.catalog_content_hash,
        "retrieval_score": plan.retrieval_score,
        "timeout_ms": plan.timeout_ms,
        "row_count": row_count,
    })
}

/// Response `limitation` yang menyatakan sebabnya, bukan dokumen kosong
/// (responses.md: hasilnya response `kind='limitation'`, bukan job yang gagal
/// diam-diam).
fn limitation_response(reason: &str, explanation: &str, request_text: &str) -> SettledResponse {
    let blocks = serde_json::json!([compose::block(
        reason,
        "limitation",
        &[],
        serde_json::json!({
            "title": "Request not answered",
            "body": explanation,
            "request_echo": request_text,
        }),
    )]);

    SettledResponse {
        kind: "limitation",
        // engine.md: Completed + Unsupported wajib berpasangan dengan
        // completeness Unknown — tidak ada klaim kelengkapan atas data sumber.
        outcome: "Unsupported",
        completeness: "Unknown",
        completeness_reason: reason.to_string(),
        response_hash: hash_blocks(&blocks),
        evidence: serde_json::json!({}),
        blocks,
    }
}

/// Penolakan kebijakan: bentuknya sama dengan limitation, outcome-nya
/// `BlockedByPolicy` (engine.md: `Completed` + `BlockedByPolicy` ⇒ `Unknown`).
fn blocked_response(reason: &str, explanation: &str, request_text: &str) -> SettledResponse {
    SettledResponse {
        outcome: "BlockedByPolicy",
        ..limitation_response(reason, explanation, request_text)
    }
}

/// Hash isi response. Audit menyimpan hash, bukan isinya (migrasi 6).
fn hash_blocks(blocks: &serde_json::Value) -> String {
    hex::encode(Sha256::digest(blocks.to_string().as_bytes()))
}

/// Identitas worker untuk `lease_owner`: cukup untuk menjawab "proses mana yang
/// memegang job ini" saat investigasi.
fn worker_identity() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown-host".to_string());
    format!("{host}/{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_response_matches_engine_matrix() {
        let response = limitation_response(
            "no_capability_matched",
            "tidak tercakup",
            "berapa total portfolio?",
        );

        assert_eq!(response.kind, "limitation");
        assert_eq!(response.outcome, "Unsupported");
        // Completed + Unsupported dengan completeness selain Unknown dilarang.
        assert_eq!(response.completeness, "Unknown");
    }

    #[test]
    fn response_hash_follows_content() {
        let first = limitation_response("r", "penjelasan", "pertanyaan a");
        let second = limitation_response("r", "penjelasan", "pertanyaan b");

        assert_ne!(first.response_hash, second.response_hash);
        assert_eq!(first.response_hash.len(), 64);
    }

    #[test]
    fn limitation_block_states_the_reason() {
        let response = limitation_response("r", "penjelasan", "apa pun");
        let blocks = response.blocks.as_array().unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "limitation");
        assert!(!blocks[0]["body"].as_str().unwrap().is_empty());
    }

    /// OVR-6.6 — satu office di luar otorisasi cukup untuk menolak; subset
    /// yang sah dan permintaan kosong (= seluruh otorisasi) tidak ditolak.
    #[test]
    fn any_office_outside_authorization_is_widening() {
        assert_eq!(
            unauthorized_offices(&[1, 999_999], &[1, 2, 3]),
            vec![999_999]
        );
        assert!(unauthorized_offices(&[2, 3], &[1, 2, 3]).is_empty());
        assert!(unauthorized_offices(&[], &[1, 2, 3]).is_empty());
    }

    #[test]
    fn blocked_response_matches_engine_matrix() {
        let response = blocked_response(OFFICE_SCOPE_NOT_AUTHORIZED, "ditolak", "apa pun");
        assert_eq!(response.kind, "limitation");
        assert_eq!(response.outcome, "BlockedByPolicy");
        assert_eq!(response.completeness, "Unknown");
        assert_eq!(response.completeness_reason, OFFICE_SCOPE_NOT_AUTHORIZED);
    }

    /// FIN-133 — handle hasil yang terkena row cap tidak boleh tampil
    /// `Complete` dengan total = cap: klaimnya `Partial`, totalnya tidak
    /// diketahui, dan set tersimpan tidak disebut terpotong.
    #[test]
    fn row_capped_handle_is_partial_with_unknown_total() {
        let reached = Some(compose::RowCapReached { row_cap: 100 });

        assert_eq!(
            handle_claim(None, Some(100), reached),
            HandleClaim {
                completeness: "Partial",
                completeness_reason: Some(compose::ROW_CAP_REACHED),
                row_count_total: None,
            }
        );
    }

    /// Cap simpan dan row cap bersamaan: alasan cap simpan yang disebut,
    /// total tetap tidak diketahui. Tanpa keduanya: `Complete`, total utuh.
    #[test]
    fn storage_cap_reason_wins_and_uncapped_handle_stays_complete() {
        let reached = Some(compose::RowCapReached { row_cap: 100 });

        assert_eq!(
            handle_claim(Some(dataset::ROW_CAP_REASON), Some(100), reached),
            HandleClaim {
                completeness: "Partial",
                completeness_reason: Some(dataset::ROW_CAP_REASON),
                row_count_total: None,
            }
        );
        assert_eq!(
            handle_claim(Some(dataset::BYTE_CAP_REASON), Some(7), None),
            HandleClaim {
                completeness: "Partial",
                completeness_reason: Some(dataset::BYTE_CAP_REASON),
                row_count_total: Some(7),
            }
        );
        assert_eq!(
            handle_claim(None, Some(7), None),
            HandleClaim {
                completeness: "Complete",
                completeness_reason: None,
                row_count_total: Some(7),
            }
        );
    }
}
