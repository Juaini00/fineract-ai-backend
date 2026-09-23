-- params: {"limit": 10}
-- Ranking nasabah dengan rekening tabungan aktif terbanyak, LATERAL per
-- klien alih-alih JOIN+GROUP BY datar seperti file queries aslinya.
SELECT
    cl.id AS client_id,
    o.id AS office_id,
    o.name AS office_name,
    cnt.n AS account_count
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
JOIN LATERAL (
    SELECT count(*) AS n FROM m_savings_account s
    WHERE s.client_id = cl.id AND s.status_enum = 300
) cnt ON true
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
  AND cnt.n > 0
ORDER BY cnt.n DESC, cl.id ASC
LIMIT 10
