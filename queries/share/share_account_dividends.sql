-- SHARE-6: share account dividends (grain: dividend_detail_id).
-- Surfaces dividend distributions joined to payout event for period dates (1:1 PK join, anti-fanout).
-- Paid dividend requires status = posted AND savings_transaction_id IS NOT NULL; both components surfaced.
-- No date column exists on m_share_account_dividend_details; query is scoped by office_ids and limit only.
-- No future-dividend estimates or mark-to-market calculations.
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sadd.id AS dividend_detail_id,
    sadd.dividend_pay_out_id,
    sadd.account_id,
    sa.client_id,
    c.office_id,
    sadd.amount,
    sadd.status::bigint AS status,
    sadd.savings_transaction_id,
    payout.dividend_period_start_date,
    payout.dividend_period_end_date
FROM m_share_account_dividend_details sadd
JOIN m_share_account sa ON sa.id = sadd.account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_share_product_dividend_pay_out payout ON payout.id = sadd.dividend_pay_out_id
WHERE c.office_id = ANY($1::bigint[])
ORDER BY c.office_id, sa.client_id, sadd.id
LIMIT $2
