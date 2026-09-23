-- params: {"from_date": ":month_start", "to_date": ":today", "limit": null}
-- Kantor tanpa aktivitas tabungan tercatat pada rentang tanggal, terlama dulu.
SELECT
    o.id AS office_id,
    o.name AS office_name,
    o.opening_date,
    max(t.transaction_date) AS last_transaction_date,
    count(t.id)::bigint AS transaction_count
FROM m_office o
LEFT JOIN m_savings_account_transaction t
       ON t.office_id = o.id
      AND t.is_reversed = false
      AND t.transaction_date >= :'month_start'::date
      AND t.transaction_date <= :'today'::date
WHERE o.id = ANY(:'office_ids'::bigint[])
GROUP BY o.id, o.name, o.opening_date
HAVING count(t.id) = 0
ORDER BY o.opening_date ASC NULLS LAST, o.id ASC
