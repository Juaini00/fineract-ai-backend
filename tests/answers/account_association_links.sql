-- params: {"limit": 200}
-- FIN-130 LINK-3: linked accounts and portfolio account associations in authorized offices.
-- Written independently of queries/linking_resources/account_association_links.sql:
-- resolving office through either loan client or savings client via CTE.
WITH loan_scope AS (
    SELECT l.id AS loan_id, c.office_id
    FROM m_loan l
    JOIN m_client c ON c.id = l.client_id
),
savings_scope AS (
    SELECT s.id AS savings_id, c.office_id
    FROM m_savings_account s
    JOIN m_client c ON c.id = s.client_id
)
SELECT
    paa.id AS association_id,
    paa.loan_account_id,
    paa.savings_account_id,
    paa.linked_loan_account_id,
    paa.linked_savings_account_id,
    paa.association_type_enum::bigint AS association_type_enum,
    paa.is_active,
    COALESCE(ls.office_id, ss.office_id, lss.office_id, lls.office_id) AS office_id
FROM m_portfolio_account_associations paa
LEFT JOIN loan_scope ls ON ls.loan_id = paa.loan_account_id
LEFT JOIN savings_scope ss ON ss.savings_id = paa.savings_account_id
LEFT JOIN savings_scope lss ON lss.savings_id = paa.linked_savings_account_id
LEFT JOIN loan_scope lls ON lls.loan_id = paa.linked_loan_account_id
WHERE COALESCE(ls.office_id, ss.office_id, lss.office_id, lls.office_id) = ANY(:'office_ids'::bigint[])
ORDER BY office_id, paa.id
LIMIT 200
