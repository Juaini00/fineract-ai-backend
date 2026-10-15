-- D14 freshness probe (XR-ASOF): account-level Loan COB coverage over the
-- authorized scope. A loan counts as behind when its last_closed_business_date
-- is NULL or older than the COB date ($2). Statuses are the ones Fineract's own
-- COB retrieves as non-closed work (active 300, transfer in progress 303, on
-- hold 304). Population-level on purpose: it is not limited to the loans shown
-- by the analysis that cites it. Office via the owning client, else the group.
SELECT
    count(*) AS in_scope_loans,
    count(*) FILTER (
        WHERE l.last_closed_business_date IS NULL OR l.last_closed_business_date < $2::date
    ) AS loans_behind_cob,
    min(l.last_closed_business_date) AS oldest_last_closed_business_date
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
  AND l.loan_status_id IN (300, 303, 304)
