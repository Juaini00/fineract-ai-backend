-- params: {"limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-127 SAV-10: linked savings accounts and portfolio account associations in offices 2 and 3.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
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
