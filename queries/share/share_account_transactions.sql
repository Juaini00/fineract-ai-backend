-- SHARE-4 / SHARE-8: share account transactions and history/corrections (grain: transaction_id).
-- Total shares represents units, strictly distinct from money amounts.
-- unit_price is the historical price recorded at transaction time, never backfilled with current product or market price.
-- Correction evidence is surfaced through is_active; no reversal field exists on m_share_account_transactions.
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sat.id AS transaction_id,
    sat.account_id,
    sa.client_id,
    c.office_id,
    sat.transaction_date,
    sat.total_shares,
    sat.unit_price,
    sat.amount,
    sat.charge_amount,
    sat.amount_paid,
    sat.status_enum::bigint AS status_enum,
    sat.type_enum::bigint AS type_enum,
    sat.is_active
FROM m_share_account_transactions sat
JOIN m_share_account sa ON sa.id = sat.account_id
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])
  AND sat.transaction_date >= $2::date AND sat.transaction_date <= $3::date
ORDER BY c.office_id, sa.client_id, sat.transaction_date, sat.id
LIMIT $4
