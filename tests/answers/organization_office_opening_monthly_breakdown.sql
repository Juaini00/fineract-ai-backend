-- params: {"from_date": ":today-12m", "to_date": ":today"}
-- Jumlah kantor dibuka per bulan dalam 12 bulan terakhir.
SELECT
    date_trunc('month', o.opening_date)::date AS month_start,
    count(*)::bigint AS opened_office_count
FROM m_office o
WHERE o.id = ANY(:'office_ids'::bigint[])
  AND o.opening_date >= (:'today'::date - interval '12 months')::date
  AND o.opening_date <= :'today'::date
GROUP BY date_trunc('month', o.opening_date)
ORDER BY month_start ASC
