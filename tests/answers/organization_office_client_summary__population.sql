-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk organization_office_client_summary,
-- dipakai lib/answers.js::expectRowCap (FIN-133). Grain = office, jadi
-- populasi = jumlah office dalam scope (predikat identik dengan oracle
-- utama MINUS LIMIT).
SELECT count(*) AS n
FROM m_office o
WHERE o.id = ANY(:'office_ids'::bigint[])
