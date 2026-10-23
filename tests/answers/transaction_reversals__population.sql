-- params: {}
-- Full population size for transaction_reversals without row cap.
SELECT count(*) AS n
FROM (
    SELECT lt.id
    FROM m_loan_transaction lt
    WHERE lt.office_id = ANY(:'office_ids'::bigint[])

    UNION ALL

    SELECT sat.id
    FROM m_savings_account_transaction sat
    WHERE sat.office_id = ANY(:'office_ids'::bigint[])

    UNION ALL

    SELECT gl.id
    FROM acc_gl_journal_entry gl
    WHERE gl.office_id = ANY(:'office_ids'::bigint[])
) all_tx
