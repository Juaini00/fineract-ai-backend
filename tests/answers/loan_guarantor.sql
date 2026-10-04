-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-10: guarantor per loan, scope via a UNION of
-- in-scope loan ids.
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    gu.id AS guarantor_id,
    gu.loan_id,
    gu.type_enum,
    CASE WHEN gu.type_enum = 1 THEN 'customer'
         WHEN gu.type_enum = 2 THEN 'staff'
         WHEN gu.type_enum = 3 THEN 'external'
         WHEN gu.type_enum = 4 THEN 'group'
         ELSE 'other'
    END AS guarantor_type,
    gu.entity_id,
    gu.firstname,
    gu.lastname,
    gu.is_active
FROM m_guarantor gu
JOIN scoped s ON s.id = gu.loan_id
ORDER BY gu.loan_id, gu.id
LIMIT 100
