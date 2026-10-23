-- params: {"from_date": "2025-11-01", "to_date": "2026-04-30", "limit": 200}
-- FIN-127 SAV-5: savings transactions breakdown by type in authorized offices.
-- Written independently of queries/savings/savings_transactions_by_type.sql:
-- client office resolved through client_scope CTE, account currency through account_ref CTE.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
),
account_ref AS (
    SELECT id AS savings_account_id, client_id, currency_code
    FROM m_savings_account
)
SELECT
    ar.currency_code,
    sat.transaction_type_enum::bigint AS transaction_type_enum,
    COUNT(sat.id)::bigint AS transaction_count,
    COALESCE(SUM(sat.amount), 0)::numeric AS total_amount
FROM m_savings_account_transaction sat
JOIN account_ref ar ON ar.savings_account_id = sat.savings_account_id
JOIN client_scope cs ON cs.client_id = ar.client_id
WHERE sat.is_reversed = false
  AND sat.transaction_date >= DATE '2025-11-01' AND sat.transaction_date <= DATE '2026-04-30'
GROUP BY ar.currency_code, sat.transaction_type_enum
ORDER BY ar.currency_code, sat.transaction_type_enum
LIMIT 200
