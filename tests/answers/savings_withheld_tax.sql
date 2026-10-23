-- params: {"limit": 200}
-- FIN-127 SAV-9: savings accounts with withheld tax in authorized offices.
-- Written independently of queries/savings/savings_withheld_tax.sql:
-- client office resolved through client_scope CTE, tax group through tax_group_ref CTE.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
),
tax_group_ref AS (
    SELECT id AS tax_group_id, name AS tax_group_name
    FROM m_tax_group
)
SELECT
    sa.id AS account_id,
    sa.client_id,
    cs.office_id,
    sa.product_id,
    sa.withhold_tax,
    sa.tax_group_id,
    tgr.tax_group_name,
    sa.total_withhold_tax_derived,
    sa.currency_code
FROM m_savings_account sa
JOIN client_scope cs ON cs.client_id = sa.client_id
LEFT JOIN tax_group_ref tgr ON tgr.tax_group_id = sa.tax_group_id
WHERE sa.withhold_tax = true
ORDER BY cs.office_id, sa.client_id, sa.id
LIMIT 200
