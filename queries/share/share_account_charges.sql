-- SHARE-5: share account charges (grain: share_account_charge_id).
-- Master charge definition (m_charge) is kept separate from actual assessment.
-- Stored derived amounts are recorded facts; NULL values remain NULL and are never coerced to zero.
-- No date column exists on m_share_account_charge; query is scoped by office_ids and limit only.
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sac.id AS share_account_charge_id,
    sac.account_id,
    sa.client_id,
    c.office_id,
    sac.charge_id,
    ch.name AS charge_name,
    ch.currency_code AS charge_currency,
    sac.charge_time_enum::bigint AS charge_time_enum,
    sac.charge_calculation_enum::bigint AS charge_calculation_enum,
    sac.charge_payment_mode_enum::bigint AS charge_payment_mode_enum,
    sac.amount,
    sac.amount_paid_derived,
    sac.amount_waived_derived,
    sac.amount_writtenoff_derived,
    sac.amount_outstanding_derived,
    sac.is_paid_derived,
    sac.waived,
    sac.is_active
FROM m_share_account_charge sac
JOIN m_share_account sa ON sa.id = sac.account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_charge ch ON ch.id = sac.charge_id
WHERE c.office_id = ANY($1::bigint[])
ORDER BY c.office_id, sa.client_id, sac.id
LIMIT $2
