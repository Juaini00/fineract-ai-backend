-- Konfigurasi global — keputusan #15.
-- PII adalah sakelar global, bukan berbasis role: hari ini setiap user adalah
-- admin, sehingga kolom per-role akan bernilai sama untuk semua orang, dan
-- kolom bernilai seragam di jalur keamanan lebih buruk daripada tidak ada.
--
-- Append-only: perubahan adalah BARIS BARU, tidak pernah UPDATE. Audit harus
-- dapat menjawab "siapa menyalakan PII, kapan, dan apa alasannya".

CREATE TABLE system_settings (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key                TEXT        NOT NULL,
    value_json         JSONB       NOT NULL,
    version            INTEGER     NOT NULL,
    effective_from     TIMESTAMPTZ NOT NULL DEFAULT now(),
    changed_by_user_id UUID        REFERENCES users (id) ON DELETE NO ACTION,
    change_reason      TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT system_settings_key_version_uniq UNIQUE (key, version)
);

-- Nilai berlaku = version tertinggi per key.
CREATE INDEX system_settings_current_idx ON system_settings (key, version DESC);

-- Seed fail-closed: bila baris konfigurasi tidak ada atau tidak terbaca, PII
-- dianggap MATI. Arah default wajib ke sisi yang aman.
INSERT INTO system_settings (key, value_json, version, change_reason)
VALUES
    ('pii.enabled', 'false'::jsonb,      1, 'seed awal: fail closed'),
    ('pii.mode',    '"withhold"'::jsonb, 1, 'seed awal: kolom identitas ditahan, bukan dimasking');
