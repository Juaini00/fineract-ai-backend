-- params: {"from_date": ":today-12m", "to_date": ":today"}
-- Aktivasi nasabah per bulan dalam 12 bulan terakhir; tanggal dihitung dari
-- :'today' (bukan literal) supaya tidak basi besok, setara subtract_months planner.
SELECT
    date_trunc('month', cl.activation_date)::date AS month_start,
    count(*) AS activation_count
FROM m_client cl
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
  AND cl.activation_date >= (:'today'::date - interval '12 months')::date
  AND cl.activation_date <= :'today'::date
  AND cl.status_enum IN (300, 600)
GROUP BY 1
ORDER BY 1 ASC
