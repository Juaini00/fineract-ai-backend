-- SAV-10: linked savings accounts and portfolio account associations (grain: association_id).
-- Surfaces account linkage only, never proof of an executed transfer (LINK-4 is kept separate).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- association_type_enum mapped via AccountAssociationType.java:26-28:
-- INVALID=0, LINKED_ACCOUNT_ASSOCIATION=1, GUARANTOR_ACCOUNT_ASSOCIATION=2.
-- Returned as raw ::bigint.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    paa.id AS association_id,
    paa.savings_account_id,
    paa.loan_account_id,
    paa.linked_savings_account_id,
    paa.linked_loan_account_id,
    paa.association_type_enum::bigint AS association_type_enum,
    paa.is_active,
    sa.client_id,
    c.office_id
FROM m_portfolio_account_associations paa
JOIN m_savings_account sa ON sa.id = paa.linked_savings_account_id
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])
ORDER BY c.office_id, sa.client_id, paa.id
LIMIT $2
