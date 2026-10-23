-- LINK-8: payment type resolution and cash flag for transactions (grain: account_type, transaction_id).
-- Surfaces payment type name and is_cash flag via m_payment_detail -> m_payment_type.
-- NULL payment_detail_id is NULL-safe (LEFT JOIN) and preserves rows without dropped transactions.
-- Covers loan and savings transactions via UNION ALL with discriminator account_type.
-- Office scope is the transaction office bound inside SQL.
-- Account numbers and external IDs in m_payment_detail are never selected (pii safe).
SELECT
    'loan'::text AS account_type,
    lt.id AS transaction_id,
    lt.loan_id AS account_id,
    lt.office_id,
    lt.transaction_date,
    lt.payment_detail_id,
    pt.id::bigint AS payment_type_id,
    pt.value AS payment_type_name,
    pt.is_cash_payment AS is_cash
FROM m_loan_transaction lt
LEFT JOIN m_payment_detail pd ON pd.id = lt.payment_detail_id
LEFT JOIN m_payment_type pt ON pt.id = pd.payment_type_id
WHERE lt.office_id = ANY($1::bigint[])

UNION ALL

SELECT
    'savings'::text AS account_type,
    sat.id AS transaction_id,
    sat.savings_account_id AS account_id,
    sat.office_id,
    sat.transaction_date,
    sat.payment_detail_id,
    pt.id::bigint AS payment_type_id,
    pt.value AS payment_type_name,
    pt.is_cash_payment AS is_cash
FROM m_savings_account_transaction sat
LEFT JOIN m_payment_detail pd ON pd.id = sat.payment_detail_id
LEFT JOIN m_payment_type pt ON pt.id = pd.payment_type_id
WHERE sat.office_id = ANY($1::bigint[])

ORDER BY office_id, transaction_date DESC, account_type, transaction_id DESC
LIMIT $2
