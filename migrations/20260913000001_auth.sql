-- Auth/identity — keputusan #7: 6 tabel lama menjadi 3.
-- DIBUANG: permissions, role_permissions (nol pembaca, satu role),
--          api_keys (seluruh kolom kebijakannya sudah inert).
-- Penerbit token: Jarvis sendiri (HS256 sah karena penerbit = pemverifikasi).

CREATE TABLE users (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username          TEXT        NOT NULL UNIQUE,
    email             TEXT        UNIQUE,
    password_hash     TEXT        NOT NULL,
    full_name         TEXT,
    role              TEXT        NOT NULL CHECK (role IN ('admin')),
    -- Engsel SSO. NULL selama Jarvis yang menerbitkan token. Tanpa kolom ini,
    -- pindah ke SSO berarti mencocokkan user lewat username/email — string yang
    -- dapat berubah, dan cara klasik memberikan sesi milik orang lain.
    external_subject  TEXT        UNIQUE,
    is_active         BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login_at     TIMESTAMPTZ
);

-- Rename dari `user_sessions`. Repo ini juga punya `chat_sessions` (percakapan);
-- tabrakan nama itu menghasilkan bug ownership yang dibaca benar oleh mata dan
-- salah oleh kode.
CREATE TABLE auth_sessions (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                 UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    user_agent              TEXT,
    ip_address              INET,
    -- Snapshot entitlement. Hari ini selalu 'admin_projection'; 'fineract_derived'
    -- disiapkan untuk saat derivasi dari Fineract dipakai (#7).
    entitlements_json       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    entitlements_derived_at TIMESTAMPTZ,
    entitlements_source     TEXT        NOT NULL DEFAULT 'admin_projection'
                                        CHECK (entitlements_source IN ('admin_projection', 'fineract_derived')),
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at            TIMESTAMPTZ,
    expires_at              TIMESTAMPTZ NOT NULL,
    revoked_at              TIMESTAMPTZ
);

CREATE INDEX auth_sessions_user_id_idx ON auth_sessions (user_id);
CREATE INDEX auth_sessions_revoked_at_idx ON auth_sessions (revoked_at);
CREATE INDEX auth_sessions_expiry_live_idx ON auth_sessions (expires_at) WHERE revoked_at IS NULL;

CREATE TABLE refresh_tokens (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID        NOT NULL REFERENCES auth_sessions (id) ON DELETE CASCADE,
    user_id    UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Hanya hash yang disimpan; token mentah tidak pernah tersimpan.
    token_hash TEXT        NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX refresh_tokens_session_id_idx ON refresh_tokens (session_id);
