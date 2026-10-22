-- params: {"from_date": "2026-01-01", "to_date": "2026-03-31", "limit": 200}
-- FIN-113 D04: client charge assessments for authorized offices. Written
-- independently of queries/client/client_charge_assessments.sql: client office
-- resolved through a CTE, due date window checked via daterange containment or NULL.
-- Stored derived amounts are preserved as-is with no coercion.
WITH client_owner AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
)
SELECT cc.id AS client_charge_id,
       cc.client_id,
       co.office_id,
       cc.charge_id,
       ch.currency_code,
       cc.amount,
       cc.amount_paid_derived,
       cc.amount_waived_derived,
       cc.amount_writtenoff_derived,
       cc.amount_outstanding_derived,
       cc.charge_due_date,
       cc.charge_time_enum::bigint AS charge_time_enum,
       cc.charge_calculation_enum::bigint AS charge_calculation_enum,
       cc.is_penalty,
       cc.is_paid_derived,
       cc.waived,
       cc.is_active,
       cc.inactivated_on_date
FROM m_client_charge cc
JOIN client_owner co ON co.client_id = cc.client_id
JOIN m_charge ch ON ch.id = cc.charge_id
WHERE (cc.charge_due_date IS NULL OR daterange(DATE '2026-01-01', DATE '2026-03-31', '[]') @> cc.charge_due_date)
ORDER BY co.office_id, cc.client_id, cc.charge_due_date NULLS LAST, cc.id
LIMIT 200
