-- D01: Recomputed savings balance position as of a specified date from transaction running balance.
-- Reversed transactions are strictly excluded (is_reversed = false).
-- Picks the latest non-reversed transaction with non-null running balance at or before as_of_date ($2).
-- Accounts with no qualifying transactions at or before as_of_date are excluded from rows.
-- Office scope is enforced via owning account's client office, else group office.
-- Surrogate IDs only; external identifiers and names are excluded (PII safe).
WITH latest_tx AS (
    SELECT DISTINCT ON (st.savings_account_id)
        st.savings_account_id,
        st.transaction_date,
        st.running_balance_derived
    FROM m_savings_account_transaction st
    WHERE st.is_reversed = false
      AND st.running_balance_derived IS NOT NULL
      AND st.transaction_date <= $2::date
    ORDER BY st.savings_account_id, st.transaction_date DESC, st.id DESC
)
SELECT
    sa.id AS savings_account_id,
    sa.client_id,
    COALESCE(c.office_id, g.office_id) AS office_id,
    sa.currency_code,
    tx.running_balance_derived AS position,
    tx.transaction_date
FROM latest_tx tx
JOIN m_savings_account sa ON sa.id = tx.savings_account_id
LEFT JOIN m_client c ON c.id = sa.client_id
LEFT JOIN m_group g ON g.id = sa.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY sa.id
LIMIT $3
