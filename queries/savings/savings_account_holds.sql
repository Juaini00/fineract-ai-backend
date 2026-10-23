-- SAV-8: savings account holds and release linkage (grain: transaction_id).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- m_savings_account_transaction.office_id is surfaced as transaction_office_id (evidence only).
-- Filtered to hold and release transactions: AMOUNT_HOLD (20), AMOUNT_RELEASE (21),
-- or non-null hold_type/parent_hold_transaction_id/release_id_of_hold_amount.
-- Reversed transactions are excluded by default (is_reversed = false).
-- Types 22-26 remain raw ::bigint without invented labels.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sat.id AS transaction_id,
    sat.savings_account_id,
    sa.client_id,
    c.office_id,
    sat.office_id AS transaction_office_id,
    sat.transaction_type_enum::bigint AS transaction_type_enum,
    sat.transaction_date,
    sat.amount,
    sa.currency_code,
    sat.hold_type::bigint AS hold_type,
    sat.hold_status::bigint AS hold_status,
    sat.parent_hold_transaction_id,
    sat.release_id_of_hold_amount,
    sat.is_lien_transaction
FROM m_savings_account_transaction sat
JOIN m_savings_account sa ON sa.id = sat.savings_account_id
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])
  AND sat.is_reversed = false
  AND (sat.transaction_type_enum IN (20, 21) OR sat.hold_type IS NOT NULL OR sat.parent_hold_transaction_id IS NOT NULL OR sat.release_id_of_hold_amount IS NOT NULL)
  AND sat.transaction_date >= $2::date AND sat.transaction_date <= $3::date
ORDER BY c.office_id, sa.client_id, sat.transaction_date, sat.id
LIMIT $4
