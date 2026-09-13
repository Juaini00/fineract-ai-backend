-- Smoke test schema. Menguji PERILAKU, bukan sekadar DDL berhasil di-parse.
--
-- Jalankan terhadap database yang sudah dimigrasi:
--   psql -v ON_ERROR_STOP=1 -U <user> -d <db> -f tests/schema_smoke.sql
--
-- Setiap blok gagal keras bila invarian yang diuji rusak. Tidak ada framework,
-- tidak ada fixture — ini pemeriksaan terkecil yang gagal kalau desainnya
-- dilanggar.

\set ON_ERROR_STOP on
BEGIN;

-- ============================================================
-- T1. Seluruh 23 tabel ada
-- ============================================================
DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM information_schema.tables
    WHERE table_schema = 'public' AND table_type = 'BASE TABLE';
    IF n <> 23 THEN
        RAISE EXCEPTION 'T1 GAGAL: ada % tabel, seharusnya 23', n;
    END IF;
END $$;

-- Data dasar
INSERT INTO users (id, username, password_hash, role)
VALUES ('11111111-1111-1111-1111-111111111111', 'admin', 'x', 'admin');

INSERT INTO chat_sessions (id, owner_user_id)
VALUES ('22222222-2222-2222-2222-222222222222', '11111111-1111-1111-1111-111111111111');

INSERT INTO chat_jobs (id, session_id, owner_user_id, request_text, lifecycle)
VALUES ('33333333-3333-3333-3333-333333333333', '22222222-2222-2222-2222-222222222222',
        '11111111-1111-1111-1111-111111111111', 'berapa nasabah aktif?', 'Running');

-- ============================================================
-- T2. Satu job nonterminal per session (#13) -> job kedua ditolak
-- ============================================================
DO $$
BEGIN
    INSERT INTO chat_jobs (session_id, owner_user_id, request_text, lifecycle)
    VALUES ('22222222-2222-2222-2222-222222222222',
            '11111111-1111-1111-1111-111111111111', 'pertanyaan kedua', 'Queued');
    RAISE EXCEPTION 'T2 GAGAL: job nonterminal kedua seharusnya ditolak';
EXCEPTION WHEN unique_violation THEN
    NULL; -- benar: dipetakan ke 409
END $$;

-- ============================================================
-- T3. Lifecycle terminal WAJIB punya outcome (#1)
-- ============================================================
DO $$
BEGIN
    UPDATE chat_jobs SET lifecycle = 'Completed'
    WHERE id = '33333333-3333-3333-3333-333333333333';
    RAISE EXCEPTION 'T3 GAGAL: Completed tanpa outcome seharusnya ditolak';
EXCEPTION WHEN check_violation THEN
    NULL;
END $$;

-- ============================================================
-- T4. Teks bebas pada refine_search tidak boleh membawa binding (K1)
-- ============================================================
INSERT INTO clarification_forms (id, job_id, clarification_id, revision, fields_json)
VALUES ('44444444-4444-4444-4444-444444444444', '33333333-3333-3333-3333-333333333333',
        '55555555-5555-5555-5555-555555555555', 1, '{}'::jsonb);

DO $$
BEGIN
    INSERT INTO clarification_answers (form_id, field_id, answer_kind, raw_text, binding_json, provenance)
    VALUES ('44444444-4444-4444-4444-444444444444', 'client', 'refine_search',
            'budi', '{"client_id":10231}'::jsonb, 'user_confirmed');
    RAISE EXCEPTION 'T4 GAGAL: refine_search dengan binding seharusnya ditolak';
EXCEPTION WHEN check_violation THEN
    NULL;
END $$;

-- Jawaban auto-bind yang sah (K5)
INSERT INTO clarification_answers (form_id, field_id, answer_kind, binding_json, provenance)
VALUES ('44444444-4444-4444-4444-444444444444', 'account', 'option_id',
        '{"savings_id":7}'::jsonb, 'resolver_unique');

-- ============================================================
-- T5. Maksimal satu form 'open' per job (#8)
-- ============================================================
DO $$
BEGIN
    INSERT INTO clarification_forms (job_id, clarification_id, revision, fields_json)
    VALUES ('33333333-3333-3333-3333-333333333333',
            '66666666-6666-6666-6666-666666666666', 1, '{}'::jsonb);
    RAISE EXCEPTION 'T5 GAGAL: form open kedua seharusnya ditolak';
EXCEPTION WHEN unique_violation THEN
    NULL;
END $$;

-- ============================================================
-- T6. session_memory mustahil tanpa response durable (K4)
-- ============================================================
DO $$
BEGIN
    INSERT INTO session_memory (session_id, owner_user_id, session_seq, kind, fact_json,
                                source_job_id, source_response_version, completeness)
    VALUES ('22222222-2222-2222-2222-222222222222', '11111111-1111-1111-1111-111111111111',
            1, 'PriorResult', '{"inline":true}'::jsonb,
            '33333333-3333-3333-3333-333333333333', 99, 'Complete');
    RAISE EXCEPTION 'T6 GAGAL: memori tanpa job_responses seharusnya ditolak';
EXCEPTION WHEN foreign_key_violation THEN
    NULL;
END $$;

-- Response commit, lalu promosi memori menjadi sah
INSERT INTO job_responses (job_id, response_version, kind, completeness,
                           blocks_json, validation_status, response_hash)
VALUES ('33333333-3333-3333-3333-333333333333', 1, 'analysis', 'Partial',
        '[]'::jsonb, 'passed', 'hash-abc');

INSERT INTO session_memory (session_id, owner_user_id, session_seq, kind, fact_json,
                            source_job_id, source_response_version, completeness)
VALUES ('22222222-2222-2222-2222-222222222222', '11111111-1111-1111-1111-111111111111',
        1, 'PriorResult', '{"inline":true}'::jsonb,
        '33333333-3333-3333-3333-333333333333', 1, 'Partial');

-- ============================================================
-- T7. Maksimal satu ActiveScope valid per session (#5)
-- ============================================================
INSERT INTO session_memory (session_id, owner_user_id, session_seq, kind, fact_json,
                            source_job_id, source_response_version, completeness)
VALUES ('22222222-2222-2222-2222-222222222222', '11111111-1111-1111-1111-111111111111',
        2, 'ActiveScope', '{"scope_kind":"requested_filter"}'::jsonb,
        '33333333-3333-3333-3333-333333333333', 1, 'Complete');

DO $$
BEGIN
    INSERT INTO session_memory (session_id, owner_user_id, session_seq, kind, fact_json,
                                source_job_id, source_response_version, completeness)
    VALUES ('22222222-2222-2222-2222-222222222222', '11111111-1111-1111-1111-111111111111',
            3, 'ActiveScope', '{"scope_kind":"requested_filter"}'::jsonb,
            '33333333-3333-3333-3333-333333333333', 1, 'Complete');
    RAISE EXCEPTION 'T7 GAGAL: ActiveScope valid kedua seharusnya ditolak';
EXCEPTION WHEN unique_violation THEN
    NULL;
END $$;

-- ============================================================
-- T8. audit_events: UPDATE ditolak, DELETE diizinkan
--     (trigger lama mencakup DELETE dan itu membuat purge retensi mustahil)
-- ============================================================
INSERT INTO audit_events (id, actor_kind, stage, action, result, job_id)
VALUES ('77777777-7777-7777-7777-777777777777', 'user', 'accept', 'job.created', 'ok',
        '33333333-3333-3333-3333-333333333333');

DO $$
BEGIN
    UPDATE audit_events SET action = 'diubah' WHERE id = '77777777-7777-7777-7777-777777777777';
    RAISE EXCEPTION 'T8a GAGAL: UPDATE audit seharusnya ditolak';
EXCEPTION WHEN raise_exception THEN
    IF SQLERRM LIKE 'T8a GAGAL%' THEN RAISE; END IF; -- teruskan kegagalan tes yang sesungguhnya
END $$;

DO $$
DECLARE n INT;
BEGIN
    DELETE FROM audit_events WHERE id = '77777777-7777-7777-7777-777777777777';
    GET DIAGNOSTICS n = ROW_COUNT;
    IF n <> 1 THEN RAISE EXCEPTION 'T8b GAGAL: DELETE audit harus diizinkan untuk purge retensi'; END IF;
END $$;

-- Baris audit lain, untuk membuktikan audit selamat dari penghapusan session
INSERT INTO audit_events (actor_kind, stage, action, result,
                          session_id, job_id)
VALUES ('user', 'commit', 'response.committed', 'ok',
        '22222222-2222-2222-2222-222222222222', '33333333-3333-3333-3333-333333333333');

-- ============================================================
-- T9. INTI: menghapus session harus BERHASIL
--     session_memory dan chat_jobs sama-sama anak CASCADE dari chat_sessions.
--     Dengan RESTRICT, penghapusan ini akan GAGAL karena urutan jalur cascade
--     tidak dijamin PostgreSQL. Dengan NO ACTION (diperiksa di akhir statement)
--     ia berhasil. Inilah cacat yang ditemukan saat review desain.
-- ============================================================
DELETE FROM chat_sessions WHERE id = '22222222-2222-2222-2222-222222222222';

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM chat_jobs;
    IF n <> 0 THEN RAISE EXCEPTION 'T9 GAGAL: job seharusnya ikut CASCADE'; END IF;
    SELECT count(*) INTO n FROM session_memory;
    IF n <> 0 THEN RAISE EXCEPTION 'T9 GAGAL: memori seharusnya ikut CASCADE'; END IF;
    SELECT count(*) INTO n FROM clarification_forms;
    IF n <> 0 THEN RAISE EXCEPTION 'T9 GAGAL: form seharusnya ikut CASCADE'; END IF;
    SELECT count(*) INTO n FROM job_responses;
    IF n <> 0 THEN RAISE EXCEPTION 'T9 GAGAL: response seharusnya ikut CASCADE'; END IF;
    -- Audit TIDAK ikut terhapus (invarian I8). job_id menggantung adalah NORMAL.
    SELECT count(*) INTO n FROM audit_events;
    IF n <> 1 THEN RAISE EXCEPTION 'T9 GAGAL: audit tidak boleh ikut terhapus, ada % baris', n; END IF;
END $$;

-- ============================================================
-- T10. Seed konfigurasi PII fail-closed (#15)
-- ============================================================
DO $$
DECLARE v JSONB;
BEGIN
    SELECT value_json INTO v FROM system_settings
    WHERE key = 'pii.enabled' ORDER BY version DESC LIMIT 1;
    IF v <> 'false'::jsonb THEN
        RAISE EXCEPTION 'T10 GAGAL: pii.enabled harus false (fail closed), dapat %', v;
    END IF;
END $$;

-- ============================================================
-- T11. Kurs: pasangan mata uang sama ditolak; koreksi = baris baru
-- ============================================================
DO $$
BEGIN
    INSERT INTO exchange_rates (from_currency_code, to_currency_code, rate, rate_type,
                                effective_date, source)
    VALUES ('IDR', 'IDR', 1, 'closing', DATE '2026-09-01', 'manual');
    RAISE EXCEPTION 'T11 GAGAL: pasangan mata uang identik seharusnya ditolak';
EXCEPTION WHEN check_violation THEN
    NULL;
END $$;

INSERT INTO exchange_rates (from_currency_code, to_currency_code, rate, rate_type,
                            effective_date, source, captured_at)
VALUES ('USD', 'IDR', 16250.0000000000, 'closing', DATE '2026-09-01', 'manual', now()),
       ('USD', 'IDR', 16275.0000000000, 'closing', DATE '2026-09-01', 'manual', now() + interval '1 minute');

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM exchange_rates
    WHERE from_currency_code = 'USD' AND to_currency_code = 'IDR'
      AND rate_type = 'closing' AND effective_date = DATE '2026-09-01';
    IF n <> 2 THEN RAISE EXCEPTION 'T11 GAGAL: koreksi harus menjadi baris baru, ada %', n; END IF;
END $$;

ROLLBACK;

\echo '=== SEMUA SMOKE TEST LULUS ==='
