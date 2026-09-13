-- Katalog pengetahuan (#7) dan registry kurs (#14).

CREATE EXTENSION IF NOT EXISTS vector;

-- APPEND-ONLY. Baris versi yang dirujuk job_plans.contract_versions_json atau
-- job_node_runs.provenance_json TIDAK BOLEH PERNAH DIHAPUS — tanpa itu,
-- pertanyaan investigasi "prosa kontrak mana yang dilihat planner" tidak
-- terjawab (PRD §10).
--
-- Identitas nyata adalah content_hash, bukan `version`: di sistem lama kolom
-- version literal selalu berisi "local". Audit wajib merujuk id/content_hash,
-- JANGAN PERNAH "versi terbaru menurut synced_at".
CREATE TABLE knowledge_catalog_versions (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version              TEXT,
    content_hash         TEXT        NOT NULL UNIQUE,
    status               TEXT        NOT NULL
                                     CHECK (status IN ('loaded','validated','indexed','embedded','failed')),
    document_count       INTEGER     NOT NULL DEFAULT 0,
    embedding_model      TEXT,
    embedding_dimensions INTEGER,
    -- Pembanding untuk aturan fail-closed: bila model/dimensi/input_type saat
    -- query tidak sama dengan yang terindeks, arm embedding ditolak dan
    -- retrieval turun ke arm leksikal (#7).
    embedding_input_type TEXT,
    metadata_json        JSONB       NOT NULL DEFAULT '{}'::jsonb,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    synced_at            TIMESTAMPTZ
);

CREATE TABLE knowledge_index (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    catalog_version_id UUID        NOT NULL REFERENCES knowledge_catalog_versions (id) ON DELETE CASCADE,
    -- analytical_contract dan measure ditambahkan untuk Mode 2 (#7).
    -- `metric` (dokumen pengetahuan Mode 1) sengaja TIDAK disatukan dengan
    -- `measure` (agregasi berdeklarasi grain milik kontrak Mode 2): menyatukannya
    -- membuat boost Mode 1 memenangkan baris Mode 2 secara kebetulan, dan tidak
    -- ada yang gagal keras.
    source_type        TEXT        NOT NULL CHECK (source_type IN (
                                       'data_area','domain','capability','query','schema',
                                       'metric','policy','response',
                                       'analytical_contract','measure')),
    source_id          TEXT        NOT NULL,
    source_path        TEXT,
    title              TEXT,
    retrieval_text     TEXT        NOT NULL CHECK (length(btrim(retrieval_text)) > 0),
    metadata_json      JSONB       NOT NULL DEFAULT '{}'::jsonb,
    content_hash       TEXT,
    embedding          vector(1024),
    embedding_model    TEXT,
    embedded_at        TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT knowledge_index_source_uniq UNIQUE (catalog_version_id, source_type, source_id)
);

CREATE INDEX knowledge_index_catalog_idx ON knowledge_index (catalog_version_id);
CREATE INDEX knowledge_index_source_idx ON knowledge_index (source_type, source_id);
CREATE INDEX knowledge_index_metadata_gin ON knowledge_index USING gin (metadata_json);

-- SENGAJA TANPA INDEX ANN. ivfflat dengan lists=100 di atas katalog beberapa
-- ratus baris, dengan ivfflat.probes default 1, hanya menyentuh ~1/100 ruang
-- vektor: tetangga yang benar dapat hilang TANPA ERROR, dan pada volume ini
-- exact scan lebih cepat. Pemicu menambah index: >10.000 baris terindeks atau
-- p95 latensi retrieval melewati ambang operasional — dan penggantinya HNSW,
-- bukan ivfflat.

-- Registry sekaligus record kurs (#14). Baris IMMUTABLE: koreksi adalah baris
-- baru ber-captured_at lebih baru, bukan UPDATE. Konsumen menyimpan
-- exchange_rate_id sehingga reproduksi tetap eksak walau registry dikoreksi.
CREATE TABLE exchange_rates (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_currency_code  TEXT        NOT NULL,
    to_currency_code    TEXT        NOT NULL,
    -- NUMERIC, tidak pernah floating point. Skala 10 menampung pasangan yang
    -- sangat timpang beserta inversnya.
    rate                NUMERIC(20,10) NOT NULL CHECK (rate > 0),
    rate_type           TEXT        NOT NULL,
    effective_date      DATE        NOT NULL,
    source              TEXT        NOT NULL,
    captured_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    captured_by_user_id UUID        REFERENCES users (id) ON DELETE NO ACTION,
    notes               TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT exchange_rates_uniq
        UNIQUE (from_currency_code, to_currency_code, rate_type, effective_date, captured_at),
    CONSTRAINT exchange_rates_distinct_currency
        CHECK (from_currency_code <> to_currency_code)
);

-- Untuk telusur/administrasi registry dan tie-break koreksi (captured_at
-- terbaru). BUKAN untuk fallback otomatis: lookup wajib exact-match pada
-- effective_date. Tidak ada fungsi "ambil kurs terdekat" — dan memang harus
-- tidak ada (D03).
CREATE INDEX exchange_rates_lookup_idx
    ON exchange_rates (from_currency_code, to_currency_code, rate_type, effective_date DESC, captured_at DESC);
