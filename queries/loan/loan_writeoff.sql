-- Recorded write-off (dataset-inventory.md LOAN-11): written-off amounts are
-- m_loan's own *_derived columns, read directly (LOAN-8 rule, never
-- recomputed from schedule/transactions). writeoff_reason joins
-- m_code_value (code WriteOffReasons). 0 rows on this deployment;
-- positive behaviour proven by a read-only fixture (knowledge/VERIFICATION.md).
-- Office-scoped via the owning loan's client-or-group office.
SELECT
    l.id AS loan_id,
    l.writtenoffon_date,
    cv.code_value AS writeoff_reason,
    l.principal_writtenoff_derived,
    l.interest_writtenoff_derived,
    l.fee_charges_writtenoff_derived,
    l.penalty_charges_writtenoff_derived,
    l.total_writtenoff_derived,
    l.currency_code
FROM m_loan l
LEFT JOIN m_code_value cv ON cv.id = l.writeoff_reason_cv_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
  AND l.writtenoffon_date IS NOT NULL
ORDER BY l.writtenoffon_date DESC, l.id
LIMIT $2
