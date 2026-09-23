-- DS-8.5 (FIN-50): chunk `BYTEA` kelak ditambahkan TANPA migrasi — cukup nilai
-- `format`/`encoding_version` baru. Skema awal (#11) menaruh payload di kolom
-- `JSONB NOT NULL`, sehingga chunk BYTEA sungguhan tetap menuntut migrasi saat
-- itu tiba (carry-over #11: "menambah kolom + nilai format baru"). Keputusan
-- owner 2026-09-23: siapkan kolomnya SEKARANG, supaya pindah encoding kelak
-- memang hanya soal nilai.
--
-- - `payload` menjadi nullable; `payload_bytes BYTEA` ditambahkan.
-- - Tepat SATU payload per chunk: chunk tanpa payload akan terbaca sebagai
--   dataset yang kehilangan baris (I5), dua payload berarti tidak ada yang tahu
--   mana yang benar.
-- - Sengaja TIDAK ada CHECK yang mengikat `format` ke kolom tertentu: itulah
--   yang akan memaksa migrasi lagi untuk setiap format baru.
--
-- Chunk yang sudah ada semuanya ber-`payload` JSONB, jadi constraint ini
-- langsung terpenuhi; tidak ada backfill.
ALTER TABLE dataset_chunks
    ALTER COLUMN payload DROP NOT NULL,
    ADD COLUMN payload_bytes BYTEA,
    ADD CONSTRAINT dataset_chunks_exactly_one_payload
        CHECK (num_nonnulls(payload, payload_bytes) = 1);
