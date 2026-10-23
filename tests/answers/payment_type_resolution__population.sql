-- params: {}
-- Full population size for payment_type_resolution without row cap.
SELECT count(*) AS n
FROM (
    SELECT lt.id
    FROM m_loan_transaction lt
    WHERE lt.office_id = ANY(:'office_ids'::bigint[])

    UNION ALL

    SELECT sat.id
    FROM m_savings_account_transaction sat
    WHERE sat.office_id = ANY(:'office_ids'::bigint[])
) all_tx
