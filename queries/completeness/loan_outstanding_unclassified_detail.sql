-- D11: Detail follow-up for loans with unclassified client sector/classification.
-- External identifiers (account_no, external_id) are excluded per PII rules.
-- Outstanding balances are read directly from m_loan's own derived columns (LOAN-8).
-- Office scope is enforced via the owning loan's client office, else group office.
SELECT
    l.id AS loan_id,
    l.client_id,
    l.currency_code,
    l.total_outstanding_derived,
    COALESCE(c.office_id, g.office_id) AS office_id
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
  AND c.client_classification_cv_id IS NULL
ORDER BY l.id
LIMIT $2
