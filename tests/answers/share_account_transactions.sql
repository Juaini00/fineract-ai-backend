-- params: {"from_date": "2025-11-01", "to_date": "2026-04-30", "limit": 200}
-- FIN-111 SHARE-4 / SHARE-8: share account transactions in authorized offices.
-- Written independently of queries/share/share_account_transactions.sql:
-- client office resolved through account_owner CTE.
WITH account_owner AS (
    SELECT sa.id AS account_id, sa.client_id, c.office_id
    FROM m_share_account sa
    JOIN m_client c ON c.id = sa.client_id
    WHERE c.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    sat.id AS transaction_id,
    sat.account_id,
    ao.client_id,
    ao.office_id,
    sat.transaction_date,
    sat.total_shares,
    sat.unit_price,
    sat.amount,
    sat.charge_amount,
    sat.amount_paid,
    sat.status_enum::bigint AS status_enum,
    sat.type_enum::bigint AS type_enum,
    sat.is_active
FROM m_share_account_transactions sat
JOIN account_owner ao ON ao.account_id = sat.account_id
WHERE sat.transaction_date >= DATE '2025-11-01' AND sat.transaction_date <= DATE '2026-04-30'
ORDER BY ao.office_id, ao.client_id, sat.transaction_date, sat.id
LIMIT 200
