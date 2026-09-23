-- params: {"from_date": ":month_start", "to_date": ":today", "limit": 10}
-- Peringkat kantor berdasar volume transaksi tabungan per mata uang (L1.1:
-- tidak pernah dijumlah lintas mata uang). Ditulis lewat CTE transaksi
-- terfilter dulu baru join office, berbeda dari
-- queries/organization/office_activity_ranking.sql yang join langsung + HAVING.
WITH txns AS (
    SELECT t.office_id, sa.currency_code, t.transaction_type_enum, t.amount
    FROM m_savings_account_transaction t
    JOIN m_savings_account sa ON sa.id = t.savings_account_id
    WHERE t.is_reversed = false
      AND t.transaction_date >= :'month_start'::date
      AND t.transaction_date <= :'today'::date
      AND t.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    o.id AS office_id,
    o.name AS office_name,
    tx.currency_code,
    count(*)::bigint AS transaction_count,
    coalesce(sum(tx.amount) FILTER (WHERE tx.transaction_type_enum = 1), 0)::numeric AS deposit_total,
    coalesce(sum(tx.amount) FILTER (WHERE tx.transaction_type_enum = 2), 0)::numeric AS withdrawal_total
FROM txns tx
JOIN m_office o ON o.id = tx.office_id
GROUP BY o.id, o.name, tx.currency_code
ORDER BY transaction_count DESC, o.id ASC, tx.currency_code ASC
LIMIT 10
