SELECT
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
    l.currency_code,
    count(*) AS loan_count
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
GROUP BY l.loan_status_id, l.currency_code
ORDER BY l.loan_status_id, l.currency_code
