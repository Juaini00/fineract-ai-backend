-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk client_summary_by_office,
-- dipakai lib/answers.js::expectRowCap (FIN-133). Grain = office dengan
-- minimal satu nasabah (stat.total_count > 0), predikat identik dengan
-- oracle utama MINUS LIMIT.
SELECT count(*) AS n
FROM m_office o
JOIN LATERAL (
    SELECT count(*) AS total_count
    FROM m_client cl WHERE cl.office_id = o.id
) stat ON true
WHERE o.id = ANY(:'office_ids'::bigint[])
  AND stat.total_count > 0
