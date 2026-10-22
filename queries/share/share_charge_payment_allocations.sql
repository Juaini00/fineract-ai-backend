-- SHARE-5: share charge payment allocations (grain: allocation_id).
-- Allocation grain is one m_share_account_charge_paid_by row.
-- pb.amount is the only money measure, never summing transactions to avoid fan-out.
-- share_transaction_id and charge_transaction_id are surfaced as evidence columns.
-- No reversal field exists on m_share_account_transactions; correction follows is_active.
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    pb.id AS allocation_id,
    pb.charge_transaction_id AS share_account_charge_id,
    pb.share_transaction_id,
    sa.id AS account_id,
    sa.client_id,
    c.office_id,
    ch.currency_code AS charge_currency,
    pb.amount AS allocation_amount,
    sac.charge_time_enum::bigint AS charge_time_enum,
    sat.transaction_date,
    sat.type_enum::bigint AS transaction_type_enum,
    sat.is_active
FROM m_share_account_charge_paid_by pb
JOIN m_share_account_charge sac ON sac.id = pb.charge_transaction_id
JOIN m_share_account sa ON sa.id = sac.account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_charge ch ON ch.id = sac.charge_id
LEFT JOIN m_share_account_transactions sat ON sat.id = pb.share_transaction_id
WHERE c.office_id = ANY($1::bigint[])
ORDER BY c.office_id, sa.client_id, pb.id
LIMIT $2
