-- FDRD-3: recorded deposit transactions on FD/RD accounts near submission/activation (grain: transaction_id).
-- "Funding" is a business reading of initial deposit transactions near activation, NOT a separate
-- Fineract event type. Transaction type is deposit (1), reversal flag is checked (NOT is_reversed).
-- Office authorization is governed by the client's office (m_client.office_id).
-- Account numbers and external IDs are never selected.
SELECT
    sat.id AS transaction_id,
    sa.id AS savings_account_id,
    sa.client_id,
    c.office_id,
    sa.deposit_type_enum::bigint AS deposit_type_enum,
    sa.currency_code,
    sat.transaction_date,
    sat.amount,
    sa.submittedon_date,
    sa.activatedon_date,
    sat.is_reversed
FROM m_savings_account_transaction sat
JOIN m_savings_account sa ON sat.savings_account_id = sa.id
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.deposit_type_enum IN (200, 300)
  AND sat.transaction_type_enum = 1
  AND NOT sat.is_reversed
  AND sat.transaction_date >= $2::date AND sat.transaction_date <= $3::date
  AND sat.transaction_date >= sa.submittedon_date
  AND (sa.activatedon_date IS NULL OR sat.transaction_date <= sa.activatedon_date + INTERVAL '30 days')
ORDER BY c.office_id, sa.client_id, sat.transaction_date, sat.id
LIMIT $4
