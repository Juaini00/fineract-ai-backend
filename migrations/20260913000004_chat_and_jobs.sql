-- Session, job, plan, dataset, node ledger, event, response.
-- Keputusan #1, #2, #3, #10, #11, #12, #13.

CREATE TABLE chat_sessions (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_user_id            UUID        NOT NULL REFERENCES users (id) ON DELETE NO ACTION,
    title                    TEXT,
    status                   TEXT        NOT NULL DEFAULT 'active'
                                         CHECK (status IN ('active','archived','expired')),
    -- Summary memori (#5). Dihitung DI LUAR transaksi commit karena butuh
    -- panggilan LLM (invarian I1). Gagal -> status 'stale', watermark tidak
    -- bergerak, percobaan berikutnya melipat semuanya.
    memory_summary_text      TEXT,
    memory_summary_version   INTEGER     NOT NULL DEFAULT 0,
    memory_summary_watermark BIGINT      NOT NULL DEFAULT 0,
    memory_summary_status    TEXT        NOT NULL DEFAULT 'current'
                                         CHECK (memory_summary_status IN ('current','stale','failed')),
    memory_summary_updated_at TIMESTAMPTZ,
    -- Alokator session_seq untuk session_memory (invarian I3): row lock baris
    -- ini adalah alokatornya. Sequence PostgreSQL tidak dipakai karena
    -- meninggalkan lubang saat rollback, dan lubang membuat watermark tidak
    -- dapat dipercaya.
    memory_seq_last          BIGINT      NOT NULL DEFAULT 0,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at               TIMESTAMPTZ,
    archived_at              TIMESTAMPTZ
);
-- context_json lama sengaja TIDAK ADA: tempatnya sudah didefinisikan ulang
-- sebagai tabel session_memory (#5).

CREATE INDEX chat_sessions_owner_idx ON chat_sessions (owner_user_id, updated_at DESC);
CREATE INDEX chat_sessions_status_idx ON chat_sessions (status);
CREATE INDEX chat_sessions_expiry_idx ON chat_sessions (expires_at) WHERE status <> 'archived';

-- fillfactor 85 menyediakan ruang page untuk HOT update (heartbeat).
-- WAJIB DIVERIFIKASI, bukan diasumsikan: ruang bebas 15% dibagi seluruh baris
-- dalam satu page. Pemicu: n_tup_hot_upd / n_tup_upd < 0.80. Respons berurutan:
-- (1) turunkan fillfactor ke 70, (2) BARU pisahkan job_leases 1:1.
CREATE TABLE chat_jobs (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id             UUID        NOT NULL REFERENCES chat_sessions (id) ON DELETE CASCADE,
    owner_user_id          UUID        NOT NULL,
    -- Snapshot scope + PII efektif saat accept (#15 aturan 1). Tanpa ini,
    -- laporan lama berubah maknanya ketika konfigurasi diubah.
    -- Bentuk: {"pii":{"enabled":bool,"setting_version":N},
    --          "source":"admin_projection|fineract_derived",
    --          "office_ids":[...], "fineract_tenant":"..."}
    scope_json             JSONB       NOT NULL DEFAULT '{}'::jsonb,

    request_text           TEXT        NOT NULL,
    request_json           JSONB,

    -- Tiga dimensi status terpisah (#1). Menggabungkannya adalah cara sistem
    -- lama menghasilkan "sukses tetapi jawabannya salah".
    lifecycle              TEXT        NOT NULL
                                       CHECK (lifecycle IN ('Queued','Running','WaitingForUser','Cancelling',
                                                            'Completed','Failed','Cancelled','Expired')),
    outcome                TEXT        CHECK (outcome IN ('Answered','Empty','NotFound','Unsupported',
                                                          'BlockedByPolicy','Invalid','OperationalFailure',
                                                          'SkippedByUser')),
    completeness           TEXT        CHECK (completeness IN ('Complete','Partial','Unknown')),
    completeness_reason    TEXT,
    failure_code           TEXT,

    plan_version           INTEGER,
    final_response_version INTEGER,
    -- Alokator sequence job_events (I3).
    last_event_sequence    BIGINT      NOT NULL DEFAULT 0,

    query_count            INTEGER     NOT NULL DEFAULT 0,
    model_call_count       INTEGER     NOT NULL DEFAULT 0,
    token_cost             BIGINT      NOT NULL DEFAULT 0,
    replan_count           INTEGER     NOT NULL DEFAULT 0,

    lease_owner            TEXT,
    lease_token            UUID,
    lease_expires_at       TIMESTAMPTZ,
    lease_claimed_at       TIMESTAMPTZ,
    heartbeat_at           TIMESTAMPTZ,
    cancel_requested_at    TIMESTAMPTZ,

    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at             TIMESTAMPTZ,
    waiting_since          TIMESTAMPTZ,
    terminal_at            TIMESTAMPTZ,
    -- DIHITUNG ULANG PADA SETIAP TRANSISI FASE (amandemen K3): masuk
    -- WaitingForUser -> waiting_since + CLARIFICATION_WAIT_LIMIT; resume ->
    -- now() + JOB_TTL_RUNNING. Tanpa aturan ini, setiap klarifikasi yang
    -- dijawab lebih lama dari TTL job akan di-Expired di tengah percakapan sah.
    expires_at             TIMESTAMPTZ,
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT chat_jobs_terminal_has_outcome CHECK (
        lifecycle NOT IN ('Completed','Failed','Cancelled','Expired') OR outcome IS NOT NULL
    )
) WITH (fillfactor = 85);

-- Satu job nonterminal per session (#13). Create yang bentrok -> unique
-- violation -> 409. Menambah state nonterminal kelak memerlukan migrasi index.
CREATE UNIQUE INDEX chat_jobs_one_active_per_session
    ON chat_jobs (session_id)
    WHERE lifecycle IN ('Queued','Running','WaitingForUser','Cancelling');

-- Klaim worker (FOR UPDATE SKIP LOCKED). lease_expires_at SENGAJA TIDAK
-- DI-INDEX: HOT update batal bila ada kolom ter-index yang berubah, dan
-- meng-index kolom itu akan mematikan HOT tepat pada operasi paling sering
-- (perpanjangan lease). Ia cukup menjadi predikat filter.
CREATE INDEX chat_jobs_queued_idx ON chat_jobs (lifecycle) WHERE lifecycle = 'Queued';
CREATE INDEX chat_jobs_owner_idx ON chat_jobs (owner_user_id, created_at DESC);
CREATE INDEX chat_jobs_reaper_idx ON chat_jobs (expires_at)
    WHERE lifecycle IN ('Queued','Running','WaitingForUser','Cancelling');

-- Plan sebagai SATU dokumen JSONB immutable (#12): plan kecil (1-20 node) dan
-- selalu dibaca utuh; fan-in dihitung di engine, bukan lewat SQL.
CREATE TABLE job_plans (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id                 UUID        NOT NULL REFERENCES chat_jobs (id) ON DELETE CASCADE,
    plan_version           INTEGER     NOT NULL,
    graph_json             JSONB       NOT NULL,
    graph_hash             TEXT        NOT NULL,
    -- Menyimpan catalog_version_id (UUID) + catalog_content_hash, BUKAN teks
    -- `catalog_version` yang di sistem lama literal selalu "local" (#7).
    contract_versions_json JSONB       NOT NULL DEFAULT '{}'::jsonb,
    verified_at            TIMESTAMPTZ,
    supersedes_plan_version INTEGER,
    replan_reason          TEXT,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    superseded_at          TIMESTAMPTZ,

    CONSTRAINT job_plans_version_uniq UNIQUE (job_id, plan_version)
);

-- Dataset retained (#11). Baris TIDAK dihapus saat purge — hanya chunk-nya;
-- status berpindah ke 'purged'. session_memory bergantung pada baris ini tetap
-- ada agar handle_state dapat dinyatakan.
CREATE TABLE datasets (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id              UUID        NOT NULL REFERENCES chat_jobs (id) ON DELETE CASCADE,
    node_id             TEXT,
    plan_version        INTEGER,
    session_id          UUID        NOT NULL REFERENCES chat_sessions (id) ON DELETE CASCADE,
    owner_user_id       UUID        NOT NULL,
    schema_json         JSONB       NOT NULL DEFAULT '{}'::jsonb,
    grain_json          JSONB       NOT NULL DEFAULT '{}'::jsonb,
    scope_json          JSONB       NOT NULL DEFAULT '{}'::jsonb,
    provenance_json     JSONB       NOT NULL DEFAULT '{}'::jsonb,
    row_count_available BIGINT,
    -- NULL berarti TIDAK DIKETAHUI, bukan nol (invarian I4).
    row_count_total     BIGINT,
    completeness        TEXT        NOT NULL DEFAULT 'Unknown'
                                    CHECK (completeness IN ('Complete','Partial','Unknown')),
    completeness_reason TEXT,
    -- truncated (set tersimpan) != completeness (analitik) != preview (response).
    truncated           BOOLEAN     NOT NULL DEFAULT FALSE,
    sort_key_json       JSONB       NOT NULL DEFAULT '[]'::jsonb,
    byte_size           BIGINT,
    chunk_count         INTEGER     NOT NULL DEFAULT 0,
    status              TEXT        NOT NULL DEFAULT 'building'
                                    CHECK (status IN ('building','ready','failed','expired','purged')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at          TIMESTAMPTZ,
    purged_at           TIMESTAMPTZ
);

CREATE INDEX datasets_session_idx ON datasets (session_id, created_at);
CREATE INDEX datasets_job_idx ON datasets (job_id);
CREATE INDEX datasets_expiry_idx ON datasets (expires_at) WHERE status = 'ready';

CREATE TABLE dataset_chunks (
    dataset_id       UUID    NOT NULL REFERENCES datasets (id) ON DELETE CASCADE,
    chunk_index      INTEGER NOT NULL,
    row_from         BIGINT  NOT NULL,
    row_to           BIGINT  NOT NULL,
    payload          JSONB   NOT NULL,
    row_count        INTEGER NOT NULL,
    byte_size        BIGINT,
    -- Diskriminator sejak awal agar pindah ke BYTEA terkompresi kelak cukup
    -- menambah nilai format baru; chunk lama tetap terbaca (#11).
    format           TEXT    NOT NULL DEFAULT 'json',
    encoding_version INTEGER NOT NULL DEFAULT 1,

    PRIMARY KEY (dataset_id, chunk_index)
);

-- Node ledger (#2). Rename dari chat_workflow_node_runs: workflow_id dibuang
-- pada #1 dan plan kini berversi, sehingga nama lama menyesatkan.
CREATE TABLE job_node_runs (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id                  UUID        NOT NULL REFERENCES chat_jobs (id) ON DELETE CASCADE,
    plan_version            INTEGER     NOT NULL,
    node_id                 TEXT        NOT NULL,
    node_kind               TEXT        NOT NULL
                                        CHECK (node_kind IN ('Probe','CuratedQuery','AnalyticalQuery',
                                                             'Clarify','Compose','Respond')),
    attempt                 INTEGER     NOT NULL DEFAULT 1,
    -- 'Abandoned' TERPISAH dari 'Failed': lease kedaluwarsa saat Running berarti
    -- "tidak diketahui, mungkin sudah berjalan", yang berbeda secara fundamental
    -- dari "gagal dan diketahui". PRD menolak menjanjikan exactly-once pada
    -- eksekusi eksternal; ketidakpastian harus terlihat di data (I4).
    -- 'Pending' (fan-in belum terpenuhi) terpisah dari 'Runnable' (menunggu slot).
    status                  TEXT        NOT NULL
                                        CHECK (status IN ('Pending','Runnable','Running','Completed',
                                                          'Failed','Skipped','Abandoned')),
    -- Terpisah dari status: node dapat Completed DAN Partial.
    completeness            TEXT        CHECK (completeness IN ('Complete','Partial','Unknown')),
    completeness_reason     TEXT,
    failure_code            TEXT,
    -- Binding yang BENAR-BENAR dikonsumsi; tanpa ini kelayakan reuse hanya
    -- ditebak dari plan, padahal plan dapat berubah.
    input_binding_json      JSONB,
    input_binding_hash      TEXT,
    dataset_id              UUID        REFERENCES datasets (id) ON DELETE NO ACTION,
    output_json             JSONB,
    -- capability/contract + catalog_version_id + catalog_content_hash + as_of,
    -- serta exchange_rate_id bila ada konsolidasi mata uang (#14).
    provenance_json         JSONB       NOT NULL DEFAULT '{}'::jsonb,
    -- Reuse lintas re-plan dicatat sebagai baris BARU yang menunjuk baris lama,
    -- sehingga scheduler cukup membaca ledger pada plan_version aktif.
    reused_from_node_run_id UUID        REFERENCES job_node_runs (id) ON DELETE NO ACTION,
    rows_returned           BIGINT,
    duration_ms             BIGINT,
    started_at              TIMESTAMPTZ,
    finished_at             TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT job_node_runs_attempt_uniq UNIQUE (job_id, plan_version, node_id, attempt)
);

CREATE INDEX job_node_runs_scheduler_idx ON job_node_runs (job_id, plan_version, status);
CREATE INDEX job_node_runs_dataset_idx ON job_node_runs (dataset_id);

-- Event publik (#3). PK (job_id, sequence) tanpa surrogate id: satu-satunya
-- query replay adalah WHERE job_id = ? AND sequence > cursor ORDER BY sequence,
-- yaitu index scan langsung pada PK.
-- APPEND-ONLY: tanpa UPDATE, tanpa DELETE satuan (hanya purge massal usia).
-- Publikasi Redis HANYA SETELAH COMMIT — mengirim notifikasi lebih dulu membuat
-- subscriber membaca PostgreSQL, tidak menemukan event itu, lalu menyimpulkan
-- tidak ada yang baru.
CREATE TABLE job_events (
    job_id                 UUID        NOT NULL REFERENCES chat_jobs (id) ON DELETE CASCADE,
    sequence               BIGINT      NOT NULL,
    schema_version         INTEGER     NOT NULL DEFAULT 1,
    event_type             TEXT        NOT NULL,
    occurred_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    plan_version           INTEGER,
    -- Referensi bertipe menjadi KOLOM, bukan sekadar isi payload: referensi
    -- selalu tersedia berapa pun ambang inline, sehingga ambang itu dapat
    -- diubah tanpa migrasi schema.
    node_id                TEXT,
    node_attempt           INTEGER,
    clarification_id       UUID,
    clarification_revision INTEGER,
    response_version       INTEGER,
    payload_json           JSONB,
    payload_truncated      BOOLEAN     NOT NULL DEFAULT FALSE,

    PRIMARY KEY (job_id, sequence)
);

-- BRIN: tabel append-only dengan waktu berkorelasi urutan fisik adalah kasus
-- penggunaan BRIN yang tepat — jauh lebih kecil daripada B-tree dan tidak
-- membebani jalur insert terpanas.
CREATE INDEX job_events_occurred_brin ON job_events USING brin (occurred_at);

-- Response document (#10). Immutable; versi kedua muncul hanya saat versi
-- pertama gagal validasi, dan versi yang ditolak TETAP DISIMPAN sebagai bahan
-- investigasi.
CREATE TABLE job_responses (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id                UUID        NOT NULL REFERENCES chat_jobs (id) ON DELETE CASCADE,
    response_version      INTEGER     NOT NULL,
    schema_version        INTEGER     NOT NULL DEFAULT 1,
    plan_version          INTEGER,
    kind                  TEXT        NOT NULL CHECK (kind IN ('analysis','skipped','limitation')),
    -- Snapshot saat komposisi; chat_jobs tetap otoritatif untuk lifecycle.
    outcome               TEXT,
    completeness          TEXT        NOT NULL CHECK (completeness IN ('Complete','Partial','Unknown')),
    completeness_reason   TEXT,
    blocks_json           JSONB       NOT NULL,
    evidence_json         JSONB       NOT NULL DEFAULT '{}'::jsonb,
    validation_status     TEXT        NOT NULL CHECK (validation_status IN ('passed','failed','fallback')),
    validation_report_json JSONB,
    superseded_by_version INTEGER,
    response_hash         TEXT        NOT NULL,
    composed_at           TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT job_responses_version_uniq UNIQUE (job_id, response_version)
);
