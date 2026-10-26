-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk client_office_attribution,
-- dipakai lib/answers.js::expectRowCap. Predikat identik dengan oracle utama MINUS LIMIT.
SELECT count(*) AS n
FROM m_client c
WHERE c.office_id = ANY(:'office_ids'::bigint[])
