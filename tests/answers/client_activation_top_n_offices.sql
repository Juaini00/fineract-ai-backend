-- params: {"from_date": ":month_start", "to_date": ":today", "limit": 10}
-- Top office berdasarkan aktivasi nasabah baru bulan berjalan. Tidak ada
-- aktivasi di bulan berjalan pada seed ini, jadi hasilnya sengaja kosong
-- (outcome Empty) — dibuktikan lewat jalur "tidak ada baris", bukan dipaksa.
SELECT
    o.id AS office_id,
    o.name AS office_name,
    act.n AS activation_count
FROM m_office o
JOIN LATERAL (
    SELECT count(*) AS n FROM m_client cl
    WHERE cl.office_id = o.id
      AND cl.activation_date BETWEEN :'month_start'::date AND :'today'::date
      AND cl.status_enum IN (300, 600)
) act ON true
WHERE o.id = ANY(:'office_ids'::bigint[])
  AND act.n > 0
ORDER BY act.n DESC, o.id ASC
LIMIT 10
