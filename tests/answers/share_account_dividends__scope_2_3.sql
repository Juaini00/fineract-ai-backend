-- params: {"limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-111 SHARE-6: share account dividends in offices 2 and 3.
WITH account_owner AS (
    SELECT sa.id AS account_id, sa.client_id, c.office_id
    FROM m_share_account sa
    JOIN m_client c ON c.id = sa.client_id
    WHERE c.office_id = ANY(ARRAY[2,3]::bigint[])
),
payout_ref AS (
    SELECT id AS dividend_pay_out_id, dividend_period_start_date, dividend_period_end_date
    FROM m_share_product_dividend_pay_out
)
SELECT
    sadd.id AS dividend_detail_id,
    sadd.dividend_pay_out_id,
    sadd.account_id,
    ao.client_id,
    ao.office_id,
    sadd.amount,
    sadd.status::bigint AS status,
    sadd.savings_transaction_id,
    pr.dividend_period_start_date,
    pr.dividend_period_end_date
FROM m_share_account_dividend_details sadd
JOIN account_owner ao ON ao.account_id = sadd.account_id
JOIN payout_ref pr ON pr.dividend_pay_out_id = sadd.dividend_pay_out_id
ORDER BY ao.office_id, ao.client_id, sadd.id
LIMIT 200
