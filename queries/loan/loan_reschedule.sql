-- Recorded reschedule requests (dataset-inventory.md LOAN-11), evidence-backed
-- via m_loan_reschedule_request -- never inferred from balance deltas.
-- status_enum per Apache Fineract LoanRescheduleRequestStatus: 100=pending,
-- 200=approved, 300=rejected. reschedule_reason joins m_code_value
-- (code LoanRescheduleReason). Office-scoped via the owning loan's
-- client-or-group office.
SELECT
    r.id AS reschedule_request_id,
    r.loan_id,
    r.status_enum,
    CASE r.status_enum
        WHEN 100 THEN 'pending'
        WHEN 200 THEN 'approved'
        WHEN 300 THEN 'rejected'
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
JOIN m_loan l ON l.id = r.loan_id
LEFT JOIN m_code_value cv ON cv.id = r.reschedule_reason_cv_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY r.loan_id, r.submitted_on_date DESC, r.id DESC
LIMIT $2
