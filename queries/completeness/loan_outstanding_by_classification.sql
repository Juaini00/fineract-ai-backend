-- D11: Loan completeness analysis by classification bucket and currency.
-- Unclassified clients are explicitly grouped under 'Sektor belum tercatat',
-- never dropped or coerced to zero (invariant I4, I5).
-- Outstanding balances are read directly from m_loan's own derived columns (LOAN-8).
-- Office scope is enforced via the owning loan's client office, else group office.
SELECT
    COALESCE(cv.code_value, 'Sektor belum tercatat') AS classification,
    l.currency_code,
    COUNT(DISTINCT l.client_id) AS client_count,
    COUNT(l.id) AS loan_count,
    COALESCE(SUM(l.total_outstanding_derived), 0) AS outstanding_total
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
LEFT JOIN m_code_value cv ON cv.id = c.client_classification_cv_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
GROUP BY COALESCE(cv.code_value, 'Sektor belum tercatat'), l.currency_code
ORDER BY classification, l.currency_code
