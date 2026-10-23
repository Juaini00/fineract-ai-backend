-- LINK-3: linked accounts and portfolio account associations (grain: association_id).
-- Surfaces account linkage only, never proof of an executed transfer (LINK-4 is kept separate).
-- Office scope is resolved through the loan client or the savings client (NULL-safe per side).
-- association_type_enum mapped via AccountAssociationType.java:26-28:
-- INVALID=0, LINKED_ACCOUNT_ASSOCIATION=1, GUARANTOR_ACCOUNT_ASSOCIATION=2.
-- Returned as raw ::bigint.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    paa.id AS association_id,
    paa.loan_account_id,
    paa.savings_account_id,
    paa.linked_loan_account_id,
    paa.linked_savings_account_id,
    paa.association_type_enum::bigint AS association_type_enum,
    paa.is_active,
    COALESCE(cl.office_id, cs.office_id, cls.office_id, cll.office_id) AS office_id
FROM m_portfolio_account_associations paa
LEFT JOIN m_loan l ON l.id = paa.loan_account_id
LEFT JOIN m_client cl ON cl.id = l.client_id
LEFT JOIN m_savings_account s ON s.id = paa.savings_account_id
LEFT JOIN m_client cs ON cs.id = s.client_id
LEFT JOIN m_savings_account ls ON ls.id = paa.linked_savings_account_id
LEFT JOIN m_client cls ON cls.id = ls.client_id
LEFT JOIN m_loan ll ON ll.id = paa.linked_loan_account_id
LEFT JOIN m_client cll ON cll.id = ll.client_id
WHERE (
    (cl.office_id IS NOT NULL AND cl.office_id = ANY($1::bigint[]))
    OR (cs.office_id IS NOT NULL AND cs.office_id = ANY($1::bigint[]))
    OR (cls.office_id IS NOT NULL AND cls.office_id = ANY($1::bigint[]))
    OR (cll.office_id IS NOT NULL AND cll.office_id = ANY($1::bigint[]))
)
ORDER BY office_id, paa.id
LIMIT $2
