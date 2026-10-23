-- params: {"from_date": "2025-11-01", "to_date": "2026-04-30", "limit": 200}
-- FIN-127 SAV-2: savings account lifecycle in authorized offices.
-- Written independently of queries/savings/savings_account_lifecycle.sql:
-- client office resolved through client_scope CTE, product details through product_ref CTE.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
),
product_ref AS (
    SELECT id AS product_id, name AS product_name
    FROM m_savings_product
)
SELECT
    sa.id AS account_id,
    sa.client_id,
    cs.office_id,
    sa.product_id,
    pr.product_name,
    sa.status_enum::bigint AS status_enum,
    sa.sub_status_enum::bigint AS sub_status_enum,
    sa.reason_for_block,
    sa.submittedon_date,
    sa.approvedon_date,
    sa.rejectedon_date,
    sa.withdrawnon_date,
    sa.activatedon_date,
    sa.closedon_date,
    sa.currency_code
FROM m_savings_account sa
JOIN client_scope cs ON cs.client_id = sa.client_id
JOIN product_ref pr ON pr.product_id = sa.product_id
WHERE sa.submittedon_date >= DATE '2025-11-01' AND sa.submittedon_date <= DATE '2026-04-30'
ORDER BY cs.office_id, sa.client_id, sa.id
LIMIT 200
