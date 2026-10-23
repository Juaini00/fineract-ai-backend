-- params: {"limit": 200}
-- FIN-127 SAV-10: linked savings accounts and portfolio account associations in authorized offices.
-- Written independently of queries/savings/savings_linked_accounts.sql:
-- client office resolved through client_scope CTE, linked savings details through savings_ref CTE.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
),
savings_ref AS (
    SELECT id AS savings_account_id, client_id
    FROM m_savings_account
)
SELECT
    paa.id AS association_id,
    paa.savings_account_id,
    paa.loan_account_id,
    paa.linked_savings_account_id,
    paa.linked_loan_account_id,
    paa.association_type_enum::bigint AS association_type_enum,
    paa.is_active,
    sr.client_id,
    cs.office_id
FROM m_portfolio_account_associations paa
JOIN savings_ref sr ON sr.savings_account_id = paa.linked_savings_account_id
JOIN client_scope cs ON cs.client_id = sr.client_id
ORDER BY cs.office_id, sr.client_id, paa.id
LIMIT 200
