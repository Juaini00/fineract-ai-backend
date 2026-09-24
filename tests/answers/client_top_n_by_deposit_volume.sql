-- params: {"from_date": ":month_start", "to_date": ":today", "currency_code": null, "limit": 10}
-- Total setoran per nasabah+mata uang dalam rentang tanggal, LATERAL per
-- klien alih-alih GROUP BY datar seperti queries/client/top_n_by_deposit_volume.sql.
SELECT
    cl.id AS client_id,
    o.id AS office_id,
    o.name AS office_name,
    dep.currency_code,
    dep.n AS deposit_count,
    dep.total AS total_deposit
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
JOIN LATERAL (
    SELECT s.currency_code, count(t.id) AS n, coalesce(sum(t.amount), 0) AS total
    FROM m_savings_account s
    JOIN m_savings_account_transaction t ON t.savings_account_id = s.id
    WHERE s.client_id = cl.id
      AND t.is_reversed = false
      AND t.transaction_type_enum = 1
      AND t.transaction_date BETWEEN :'month_start'::date AND :'today'::date
    GROUP BY s.currency_code
    HAVING coalesce(sum(t.amount), 0) > 0
) dep ON true
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
ORDER BY dep.total DESC, cl.id ASC
LIMIT 10
