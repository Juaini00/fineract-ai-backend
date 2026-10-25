-- params: {"as_of_date": "2026-04-30", "limit": 100}
-- D01: Recomputed loan position as of 2026-04-30.
WITH latest_tx AS (
    SELECT DISTINCT ON (lt.loan_id)
        lt.loan_id,
        lt.transaction_date,
        lt.outstanding_loan_balance_derived
    FROM m_loan_transaction lt
    WHERE lt.is_reversed = false
      AND lt.outstanding_loan_balance_derived IS NOT NULL
      AND lt.transaction_date <= '2026-04-30'::date
    ORDER BY lt.loan_id, lt.transaction_date DESC, lt.id DESC
)
SELECT
    l.id AS loan_id,
    l.client_id,
    COALESCE(c.office_id, g.office_id) AS office_id,
    l.currency_code,
    tx.outstanding_loan_balance_derived AS position,
    tx.transaction_date
FROM latest_tx tx
JOIN m_loan l ON l.id = tx.loan_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY(:'office_ids'::bigint[])
ORDER BY l.id
LIMIT 100
