SELECT
    l.id AS loan_id,
    l.client_id,
    l.group_id,
    l.product_id,
    l.loan_officer_id,
    l.fund_id,
    l.currency_code,
    l.loan_status_id::bigint AS loan_status_id,
    CASE l.loan_status_id
        WHEN 100 THEN 'submitted_and_pending_approval'
        WHEN 200 THEN 'approved'
        WHEN 300 THEN 'active'
        WHEN 400 THEN 'withdrawn_by_client'
        WHEN 500 THEN 'rejected'
        WHEN 600 THEN 'closed'
        WHEN 601 THEN 'written_off'
        WHEN 602 THEN 'rescheduled'
        WHEN 700 THEN 'overpaid'
    END AS loan_status,
    l.submittedon_date AS submitted_on_date,
    l.approvedon_date AS approved_on_date,
    l.disbursedon_date AS disbursed_on_date,
    l.closedon_date AS closed_on_date,
    l.rejectedon_date AS rejected_on_date,
    l.writtenoffon_date AS written_off_on_date
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
  AND l.client_id = $2::bigint
ORDER BY l.id
