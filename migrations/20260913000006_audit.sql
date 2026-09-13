-- Audit — keputusan #4. Empat tabel lama menjadi dua.
-- DIBUANG: management_audit_outbox (sistem "eksternal"-nya adalah tabel
-- PostgreSQL sebelah di transaksi yang sama — pola degenerate yang justru
-- menghapus gerbang yang diminta api.md), assistant_llm_traces dan
-- management_telemetry_counters (observability, bukan audit).

-- TANPA SATU PUN FOREIGN KEY (invarian I8). Inilah mekanisme yang membuat
-- aturan "audit tidak ikut terhapus bersama session" benar-benar berlaku:
-- rantai CASCADE tidak menyentuh tabel ini karena tidak ada jalur referensial.
--
-- job_id/session_id/dataset_id yang MENGGANTUNG setelah purge adalah KONDISI
-- NORMAL, bukan korupsi, dan TIDAK BOLEH "diperbaiki" menjadi FK oleh migrasi
-- berikutnya. ON DELETE SET NULL gaya lama juga ditolak: itu UPDATE pada baris
-- audit, yang melanggar append-only.
--
-- `id` diisi aplikasi dengan UUIDv7 agar urutan fisik mendekati urutan waktu
-- (syarat BRIN di bawah). DEFAULT hanya jaring pengaman.
CREATE TABLE audit_events (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schema_version         INTEGER     NOT NULL DEFAULT 1,
    occurred_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    duration_ms            BIGINT,

    request_id             UUID,
    actor_kind             TEXT        NOT NULL CHECK (actor_kind IN ('user','worker','reaper','system')),
    actor_user_id          UUID,
    session_id             UUID,
    job_id                 UUID,
    plan_version           INTEGER,
    node_id                TEXT,
    node_attempt           INTEGER,
    query_attempt_id       UUID,
    model_call_id          UUID,

    clarification_id       UUID,
    clarification_revision INTEGER,
    response_version       INTEGER,
    -- Hash, bukan isi response.
    response_hash          TEXT,
    dataset_id             UUID,
    graph_hash             TEXT,

    -- DESKRIPTIF, bukan state machine kedua: tidak pernah dibaca balik oleh
    -- engine untuk mengambil keputusan. Konsisten dengan membuang current_step
    -- (#1) dan phase (#3). `layer`/`blueprint_step` lama dibuang, bukan diganti
    -- nama.
    stage                  TEXT        NOT NULL CHECK (stage IN (
                                           'accept','authorize','context','plan','plan_verify','clarify',
                                           'node_execute','source_query','model_call','compose',
                                           'response_validate','commit','settle','data_access','admin')),
    action                 TEXT        NOT NULL,
    -- Hasil LANGKAH INI. Sengaja dipisah dari job_outcome: #1 sudah mengunci
    -- `outcome` sebagai kosakata job-level, dan satu nama untuk dua kosakata
    -- adalah jebakan.
    result                 TEXT        NOT NULL CHECK (result IN ('ok','denied','invalid','failed','deferred')),
    failure_code           TEXT,
    -- Kosakata #1 persis; HANYA terisi pada baris commit/settle.
    job_outcome            TEXT,
    job_completeness       TEXT,
    completeness_reason    TEXT,

    catalog_version_id     UUID,
    catalog_content_hash   TEXT,
    contract_refs_json     JSONB       NOT NULL DEFAULT '{}'::jsonb,
    -- Scope TEREDAKSI: office_ids, tenant, pii_allowed, cara penerapan, jumlah
    -- filter — metadata, bukan nilai filter.
    scope_json             JSONB       NOT NULL DEFAULT '{}'::jsonb,
    -- Keputusan terstruktur tersanitasi. SQL/prompt/stack mentah tidak pernah
    -- masuk; yang sensitif masuk audit_evidence.
    detail_json            JSONB       NOT NULL DEFAULT '{}'::jsonb
                                       CHECK (jsonb_typeof(detail_json) = 'object'),
    -- Diset saat INSERT, tidak pernah di-UPDATE. Investigator tahu ada evidence
    -- tanpa join dan tanpa hak baca ke tabel evidence.
    has_evidence           BOOLEAN     NOT NULL DEFAULT FALSE,

    CONSTRAINT audit_events_job_result_scope
        CHECK (stage IN ('commit','settle') OR (job_outcome IS NULL AND job_completeness IS NULL))
);

-- Jalur investigasi utama PRD §10: telusuri lewat identitas job.
CREATE INDEX audit_events_job_idx ON audit_events (job_id, occurred_at) WHERE job_id IS NOT NULL;
-- "Siapa mengakses data approved mana, dengan scope apa" — satu-satunya query
-- yang tidak dimulai dari job_id.
CREATE INDEX audit_events_actor_idx ON audit_events (actor_user_id, occurred_at DESC) WHERE actor_user_id IS NOT NULL;
CREATE INDEX audit_events_occurred_brin ON audit_events USING brin (occurred_at);

-- Controlled evidence. WAJIB tabel terpisah, dan alasannya BUKAN akses
-- (PostgreSQL bisa GRANT SELECT per kolom) melainkan RETENSI: pilihan sensitif
-- harus dipurge lebih cepat daripada baris auditnya, dan memurge isi sebuah
-- kolom berarti UPDATE baris audit — persis yang dilarang append-only.
-- Di tabel terpisah, purge = payload_json := NULL, purged_at := now(), dan
-- barisnya tetap menjadi bukti bahwa evidence pernah ada lalu dipurge.
CREATE TABLE audit_evidence (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    audit_event_id  UUID        NOT NULL REFERENCES audit_events (id) ON DELETE CASCADE,
    kind            TEXT        NOT NULL CHECK (kind IN ('clarification_choice','resolver_option_set',
                                                         'pii_field_access','query_parameters',
                                                         'validation_report')),
    payload_json    JSONB,
    redaction_level TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ,
    purged_at       TIMESTAMPTZ
);

CREATE INDEX audit_evidence_event_idx ON audit_evidence (audit_event_id);
CREATE INDEX audit_evidence_expiry_idx ON audit_evidence (expires_at) WHERE purged_at IS NULL;

-- Append-only: jaring KEDUA. Penegakan utamanya adalah REVOKE UPDATE, DELETE
-- dari peran aplikasi (lihat catatan di bawah).
--
-- SENGAJA HANYA BEFORE UPDATE, TIDAK UNTUK DELETE: trigger lama mencakup DELETE
-- dan itu membuat purge retensi MUSTAHIL, sekaligus bertabrakan dengan FK
-- ON DELETE SET NULL pada tabelnya sendiri sehingga penghapusan session gagal.
CREATE OR REPLACE FUNCTION audit_events_reject_update() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'audit_events bersifat append-only: UPDATE tidak diizinkan';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_events_no_update
    BEFORE UPDATE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION audit_events_reject_update();

-- CATATAN DEPLOYMENT (bukan bagian migrasi ini):
-- Penegakan utama append-only adalah pemisahan peran DB, yang harus dijalankan
-- oleh operator karena nama peran bergantung deployment:
--
--   REVOKE UPDATE, DELETE ON audit_events FROM <app_role>;
--   GRANT  DELETE ON audit_events TO <audit_purge_role>;
--   REVOKE SELECT ON audit_evidence FROM <app_role>;
--   GRANT  SELECT ON audit_evidence TO <audit_evidence_reader_role>;
--
-- Trigger di atas boleh dihapus setelah pemisahan peran terbukti berjalan;
-- ia tombol kedua, bukan sabuk pengaman permanen.
