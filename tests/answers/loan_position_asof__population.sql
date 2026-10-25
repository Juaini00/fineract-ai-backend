-- params: {}
-- Full population size of qualifying loans as of latest non-reversed transaction.
WITH latest_tx AS (
    SELECT DISTINCT ON (lt.loan_id)
        lt.loan_id,
        lt.transaction_date,
        lt.outstanding_loan_balance_derived
    FROM m_loan_transaction lt
    WHERE lt.is_reversed = false
      AND lt.outstanding_loan_balance_derived IS NOT NULL
    ORDER BY lt.loan_id, lt.transaction_date DESC, lt.id DESC
)
SELECT count(*) AS n
FROM latest_tx tx
JOIN m_loan l ON l.id = tx.loan_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY(:'office_ids'::bigint[])
