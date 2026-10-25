-- params: {}
-- Full population size of qualifying savings accounts as of latest non-reversed transaction.
WITH latest_tx AS (
    SELECT DISTINCT ON (st.savings_account_id)
        st.savings_account_id,
        st.transaction_date,
        st.running_balance_derived
    FROM m_savings_account_transaction st
    WHERE st.is_reversed = false
      AND st.running_balance_derived IS NOT NULL
    ORDER BY st.savings_account_id, st.transaction_date DESC, st.id DESC
)
SELECT count(*) AS n
FROM latest_tx tx
JOIN m_savings_account sa ON sa.id = tx.savings_account_id
LEFT JOIN m_client c ON c.id = sa.client_id
LEFT JOIN m_group g ON g.id = sa.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY(:'office_ids'::bigint[])
