-- Guarantor per loan (dataset-inventory.md LOAN-10), kept distinct from
-- collateral (see loan.loan_collateral) -- no fanout, 1:N loan -> guarantor,
-- one row per guarantor. type_enum per Apache Fineract GuarantorType:
-- 1=customer (existing client, entity_id -> m_client), 2=staff
-- (entity_id -> m_staff), 3=external (no entity_id; name/address/phone on
-- this row), 4=group (entity_id -> m_group). Name/dob/address/phone are
-- pii, masked/omitted by the existing global PII switch, never fetched
-- unconditionally. Office-scoped via the owning loan's client-or-group office.
SELECT
    gu.id AS guarantor_id,
    gu.loan_id,
    gu.type_enum,
    CASE gu.type_enum
        WHEN 1 THEN 'customer'
        WHEN 2 THEN 'staff'
        WHEN 3 THEN 'external'
        WHEN 4 THEN 'group'
        ELSE 'other'
    END AS guarantor_type,
    gu.entity_id,
    gu.firstname,
    gu.lastname,
    gu.is_active
FROM m_guarantor gu
JOIN m_loan l ON l.id = gu.loan_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY gu.loan_id, gu.id
LIMIT $2
