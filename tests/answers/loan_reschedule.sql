-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-11: recorded reschedule requests, scope via a UNION
-- of in-scope loan ids.
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    r.id AS reschedule_request_id,
    r.loan_id,
    r.status_enum,
    CASE WHEN r.status_enum = 100 THEN 'pending'
         WHEN r.status_enum = 200 THEN 'approved'
         WHEN r.status_enum = 300 THEN 'rejected'
         ELSE 'other'
    END AS status,
    r.reschedule_from_date,
    r.reschedule_from_installment,
    r.recalculate_interest,
    cv.code_value AS reschedule_reason,
    r.submitted_on_date,
    r.approved_on_date,
    r.rejected_on_date
FROM m_loan_reschedule_request r
JOIN scoped s ON s.id = r.loan_id
LEFT JOIN m_code_value cv ON cv.id = r.reschedule_reason_cv_id
ORDER BY r.loan_id, r.submitted_on_date DESC, r.id DESC
LIMIT 100
