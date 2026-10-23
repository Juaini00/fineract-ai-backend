-- params: {"from_date": "2025-11-01", "to_date": "2026-04-30", "limit": 200}
-- FIN-127 SAV-4: savings account four-measure balance detail in authorized offices.
-- Written independently of queries/savings/savings_account_balance_detail.sql:
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
    sa.currency_code,
    sa.account_balance_derived,
    sa.available_balance_derived,
    sa.total_savings_amount_on_hold,
    sa.on_hold_funds_derived,
    sa.allow_overdraft,
    sa.overdraft_limit,
    sa.submittedon_date
FROM m_savings_account sa
JOIN client_scope cs ON cs.client_id = sa.client_id
JOIN product_ref pr ON pr.product_id = sa.product_id
WHERE sa.submittedon_date >= DATE '2025-11-01' AND sa.submittedon_date <= DATE '2026-04-30'
ORDER BY cs.office_id, sa.client_id, sa.id
LIMIT 200
