-- D01: Recomputed loan position as of a specified date from transaction running balance.
-- Reversed transactions are strictly excluded (is_reversed = false).
-- Picks the latest non-reversed transaction with non-null balance at or before as_of_date ($2).
-- Accounts with no qualifying transactions at or before as_of_date are excluded from rows.
-- Office scope is enforced via owning loan's client office, else group office.
-- Surrogate IDs only; external identifiers and names are excluded (PII safe).
WITH latest_tx AS (
    SELECT DISTINCT ON (lt.loan_id)
        lt.loan_id,
        lt.transaction_date,
        lt.outstanding_loan_balance_derived
    FROM m_loan_transaction lt
    WHERE lt.is_reversed = false
      AND lt.outstanding_loan_balance_derived IS NOT NULL
      AND lt.transaction_date <= $2::date
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
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY l.id
LIMIT $3
