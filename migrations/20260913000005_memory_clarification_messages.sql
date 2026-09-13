-- Memori session, klarifikasi, riwayat pesan, idempotency.
-- Keputusan #5, #8, #6, #9 dan K5.

-- Satu tabel + diskriminator `kind`: ketiganya ditulis HANYA pada response
-- commit, dibaca SELALU bersama saat context assembly, dan berbagi kolom
-- provenance/completeness/validitas yang sama.
CREATE TABLE session_memory (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id              UUID        NOT NULL REFERENCES chat_sessions (id) ON DELETE CASCADE,
    -- Difilter pada setiap baca. Memiliki session_id saja tidak cukup.
    owner_user_id           UUID        NOT NULL,
    -- Dialokasikan lewat row lock chat_sessions.memory_seq_last (I3).
    session_seq             BIGINT      NOT NULL,
    kind                    TEXT        NOT NULL CHECK (kind IN ('PriorResult','ResolvedEntity','ActiveScope')),
    entity_key              TEXT,
    -- BERPOTENSI PII. Ikut terhapus lewat CASCADE session.
    label                   TEXT,
    fact_json               JSONB       NOT NULL,

    source_job_id           UUID        NOT NULL,
    source_plan_version     INTEGER,
    source_node_id          TEXT,
    -- K4 DITEGAKKAN STRUKTUR: baris memori mustahil ada tanpa response document
    -- yang sudah durable. Tidak ada status 'pending', tidak ada jalur tulis
    -- inkremental. Crash tanpa commit = tidak ada baris.
    source_response_version INTEGER     NOT NULL,

    provenance_json         JSONB       NOT NULL DEFAULT '{}'::jsonb,
    as_of                   TIMESTAMPTZ,
    completeness            TEXT        NOT NULL CHECK (completeness IN ('Complete','Partial','Unknown')),
    completeness_reason     TEXT,
    -- Status handle TIDAK disalin ke sini: datasets.status otoritatif. Baca
    -- selalu LEFT JOIN datasets, dan tipe hasil repository punya field
    -- handle_state NON-OPTIONAL sehingga tidak ada jalur baca yang dapat
    -- melewatkannya (#11 aturan 6).
    dataset_id              UUID        REFERENCES datasets (id) ON DELETE NO ACTION,

    -- Invalidasi = supersede, tidak pernah hard delete: baris yang dihapus
    -- menghilangkan penjelasan "kenapa jawaban berubah antar-turn" — persis
    -- pertanyaan investigasi PRD §10.
    status                  TEXT        NOT NULL DEFAULT 'valid'
                                        CHECK (status IN ('valid','superseded','invalidated','evicted')),
    superseded_by_id        UUID        REFERENCES session_memory (id) ON DELETE NO ACTION,
    invalidation_reason     TEXT        CHECK (invalidation_reason IN
                                            ('superseded_by_newer','scope_changed','contradicted','quota_evicted')),
    invalidated_at          TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT session_memory_seq_uniq UNIQUE (session_id, session_seq),
    CONSTRAINT session_memory_entity_key_required
        CHECK (kind <> 'ResolvedEntity' OR entity_key IS NOT NULL),
    CONSTRAINT session_memory_scope_shape
        CHECK (kind <> 'ActiveScope' OR (dataset_id IS NULL AND entity_key IS NULL)),
    CONSTRAINT session_memory_invalidation_complete
        CHECK (status = 'valid' OR (invalidated_at IS NOT NULL AND invalidation_reason IS NOT NULL)),

    -- FK KOMPOSIT, bukan dua FK terpisah: dua FK terpisah memungkinkan baris
    -- menunjuk job A dengan versi milik job B.
    --
    -- NO ACTION, BUKAN RESTRICT (invarian I2): session_memory dan chat_jobs
    -- sama-sama anak CASCADE dari chat_sessions, dan PostgreSQL tidak menjamin
    -- urutan antar jalur cascade. Dengan RESTRICT, bila baris job terhapus lebih
    -- dulu, pemeriksaan langsung menemukan baris memori yang belum terhapus dan
    -- SELURUH penghapusan session gagal. NO ACTION diperiksa di akhir statement,
    -- saat kedua sisi sudah terhapus — sementara menghapus job sendirian tetap
    -- gagal, sesuai niat aslinya.
    CONSTRAINT session_memory_source_response_fk
        FOREIGN KEY (source_job_id, source_response_version)
        REFERENCES job_responses (job_id, response_version)
        ON DELETE NO ACTION
);

CREATE INDEX session_memory_read_idx ON session_memory (session_id, kind) WHERE status = 'valid';
-- Maksimal satu scope aktif per session — constraint DB, bukan kode aplikasi.
CREATE UNIQUE INDEX session_memory_one_active_scope
    ON session_memory (session_id) WHERE kind = 'ActiveScope' AND status = 'valid';
CREATE UNIQUE INDEX session_memory_entity_uniq
    ON session_memory (session_id, entity_key) WHERE kind = 'ResolvedEntity' AND status = 'valid';
CREATE INDEX session_memory_dataset_idx ON session_memory (dataset_id) WHERE dataset_id IS NOT NULL;
CREATE INDEX session_memory_source_job_idx ON session_memory (source_job_id);

-- Form klarifikasi (#8). Satu baris per REVISION, immutable: form adalah apa
-- yang DILIHAT pengguna saat menjawab. Update in-place menghapus bukti tampilan
-- dan membuat cek stale-revision menjadi tebakan.
CREATE TABLE clarification_forms (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id                 UUID        NOT NULL REFERENCES chat_jobs (id) ON DELETE CASCADE,
    clarification_id       UUID        NOT NULL,
    revision               INTEGER     NOT NULL,
    plan_version           INTEGER,
    schema_version         INTEGER     NOT NULL DEFAULT 1,
    purpose                TEXT,
    -- Label deskriptif, BUKAN "step 2 of 5": jumlah tahap tidak pernah dikarang.
    stage_label            TEXT,
    fields_json            JSONB       NOT NULL,
    -- Terminal 'skipped' (K2) memakai kolom resolusi generik yang sama; tidak
    -- ada kolom maupun tabel khusus skip.
    state                  TEXT        NOT NULL DEFAULT 'open'
                                       CHECK (state IN ('open','answered','superseded','skipped',
                                                        'expired','invalidated')),
    resolved_by_user_id    UUID,
    resolved_at            TIMESTAMPTZ,
    resolution_reason      TEXT,
    superseded_by_revision INTEGER,
    expires_at             TIMESTAMPTZ,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT clarification_forms_revision_uniq UNIQUE (job_id, clarification_id, revision)
);

CREATE UNIQUE INDEX clarification_forms_one_open ON clarification_forms (job_id) WHERE state = 'open';

-- Hanya jawaban DITERIMA; yang ditolak masuk audit. Mencampurnya membuat
-- "accepted facts" tidak dapat dibaca lurus.
CREATE TABLE clarification_answers (
    form_id            UUID        NOT NULL REFERENCES clarification_forms (id) ON DELETE CASCADE,
    field_id           TEXT        NOT NULL,
    -- Aturan keamanan K1: teks bebas pada slot identitas TIDAK PERNAH menjadi
    -- binding. Pemisahan answer_kind / raw_text / binding_json adalah intinya.
    answer_kind        TEXT        NOT NULL
                                   CHECK (answer_kind IN ('option_id','typed_value','refine_search','change_intent')),
    raw_text           TEXT,
    binding_json       JSONB,
    -- K5: 'resolver_unique' (satu-satunya match, auto-bind) dan
    -- 'deterministic_parse' (frasa tanggal terurai kontrak) DIBEDAKAN dari
    -- 'user_confirmed' — "pengguna mengonfirmasi nasabah ini" adalah klaim yang
    -- berbeda secara material dari "kebetulan hanya ada satu".
    provenance         TEXT        NOT NULL
                                   CHECK (provenance IN ('user_confirmed','resolver_unique','deterministic_parse')),
    resolver_ref       TEXT,
    option_set_ref     TEXT,
    answered_by_user_id UUID,
    answered_at        TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (form_id, field_id),
    CONSTRAINT clarification_answers_no_binding_for_search
        CHECK (answer_kind NOT IN ('refine_search','change_intent') OR binding_json IS NULL)
);

-- Hanya halaman yang BENAR-BENAR DIKIRIM ke klien, bukan seluruh hasil
-- resolver. Keanggotaan != otorisasi: tabel ini hanya membuktikan "opsi ini
-- pernah kami terbitkan untuk form ini"; otorisasi dicek ulang ke sumber saat
-- submit. Memuat nama nasabah -> dipurge saat form terminal/kedaluwarsa.
CREATE TABLE clarification_options (
    form_id         UUID        NOT NULL REFERENCES clarification_forms (id) ON DELETE CASCADE,
    field_id        TEXT        NOT NULL,
    option_id       TEXT        NOT NULL,
    binding_json    JSONB       NOT NULL,
    label           TEXT,
    attributes_json JSONB       NOT NULL DEFAULT '{}'::jsonb,
    resolver_ref    TEXT,
    page_cursor     TEXT,
    issued_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- DITURUNKAN dari clarification_forms.expires_at, bukan angka independen
    -- (K4): opsi yang kedaluwarsa lebih dulu daripada form-nya membuat jawaban
    -- sah ditolak tanpa kesalahan pengguna.
    expires_at      TIMESTAMPTZ,

    PRIMARY KEY (form_id, field_id, option_id)
);

CREATE INDEX clarification_options_expiry_idx ON clarification_options (expires_at);

-- Indeks riwayat TIPIS (#6): tanpa kolom content. Konten sudah punya rumah
-- masing-masing (chat_jobs.request_text, job_responses, clarification_forms),
-- semuanya immutable/berversi. Salinan teks kedua menciptakan kelas bug yang
-- baru ditutup #10.
CREATE TABLE chat_messages (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id             UUID        NOT NULL REFERENCES chat_sessions (id) ON DELETE CASCADE,
    job_id                 UUID        NOT NULL REFERENCES chat_jobs (id) ON DELETE CASCADE,
    -- 'system' dan 'tool' DIBUANG: tidak ada penulisnya. Lifecycle job
    -- disampaikan lewat SSE (#3), eksekusi node internal di ledger (#2).
    role                   TEXT        NOT NULL CHECK (role IN ('user','assistant','clarification')),
    response_version       INTEGER,
    clarification_id       UUID,
    clarification_revision INTEGER,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT chat_messages_assistant_shape
        CHECK (role <> 'assistant' OR response_version IS NOT NULL),
    CONSTRAINT chat_messages_clarification_shape
        CHECK (role <> 'clarification' OR (clarification_id IS NOT NULL AND clarification_revision IS NOT NULL)),
    CONSTRAINT chat_messages_user_shape
        CHECK (role <> 'user' OR (response_version IS NULL AND clarification_id IS NULL))
);

-- Keyset pagination. Tanpa alokator sequence ala job_events: volume rendah, dan
-- constraint "1 job nonterminal per session" (#13) mencegah penulisan konkuren.
CREATE INDEX chat_messages_keyset_idx ON chat_messages (session_id, created_at, id);
CREATE INDEX chat_messages_job_idx ON chat_messages (job_id);

-- Idempotency (#9). Pola: at-least-once delivery + idempotent consumer =
-- effectively-once. Bukan fitur produk melainkan pelindung integritas tulis.
CREATE TABLE idempotency_keys (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Scope PER USER, tidak pernah global: kunci dibuat klien, dan ruang kunci
    -- global memungkinkan user B menerima replay acknowledgement milik user A.
    owner_user_id       UUID        NOT NULL,
    operation           TEXT        NOT NULL CHECK (operation IN ('job.create','job.respond','job.skip')),
    idempotency_key     TEXT        NOT NULL CHECK (length(idempotency_key) BETWEEN 16 AND 255),
    -- Hash payload kanonik + path param, BUKAN body mentah: payload klarifikasi
    -- dapat memuat PII dan yang perlu diketahui hanya "sama atau tidak".
    -- job_id ikut di-hash sehingga kunci yang dipakai ulang untuk job lain
    -- otomatis terdeteksi sebagai mismatch.
    request_fingerprint TEXT        NOT NULL,
    -- TANPA FK: punya TTL sendiri dan tidak ikut CASCADE session.
    target_job_id       UUID,
    status              TEXT        NOT NULL CHECK (status IN ('in_progress','completed')),
    response_status     INTEGER,
    -- Acknowledgement kecil (job_id + lifecycle), bukan hasil analisis. Retry
    -- WAJIB memperoleh job_id yang sama, jika tidak frontend kehilangan handle.
    response_body_json  JSONB,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at        TIMESTAMPTZ,
    expires_at          TIMESTAMPTZ NOT NULL,

    CONSTRAINT idempotency_keys_uniq UNIQUE (owner_user_id, operation, idempotency_key)
);

CREATE INDEX idempotency_keys_expiry_idx ON idempotency_keys (expires_at);
