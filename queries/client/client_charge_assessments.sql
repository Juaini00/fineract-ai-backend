-- D04: client charge ASSESSMENTS, one row per m_client_charge assessment row
-- (grain: client_charge_id). Master charge definition (m_charge) is separated from
-- the actual assessment. Office scope is the client's office (m_client.office_id)
-- bound inside the SQL. NULL charge_due_date means the due date is not recorded
-- and is returned as NULL (never defaulted); an assessment with no recorded due date
-- is admitted into the date window. Stored derived amounts (amount_paid_derived,
-- amount_waived_derived, amount_writtenoff_derived, amount_outstanding_derived) are
-- recorded system facts, never recomputed, and NULL stays NULL. Currency comes from
-- the charge definition master (m_charge.currency_code). Client names and free-text
-- descriptions are never selected: the client is identified by client_id only.
SELECT
    cc.id AS client_charge_id,
    cc.client_id,
    c.office_id,
    cc.charge_id,
    ch.currency_code,
    cc.amount,
    cc.amount_paid_derived,
    cc.amount_waived_derived,
    cc.amount_writtenoff_derived,
    cc.amount_outstanding_derived,
    cc.charge_due_date,
    cc.charge_time_enum::bigint AS charge_time_enum,
    cc.charge_calculation_enum::bigint AS charge_calculation_enum,
    cc.is_penalty,
    cc.is_paid_derived,
    cc.waived,
    cc.is_active,
    cc.inactivated_on_date
FROM m_client_charge cc
JOIN m_client c ON c.id = cc.client_id
JOIN m_charge ch ON ch.id = cc.charge_id
WHERE c.office_id = ANY($1::bigint[])
  AND (cc.charge_due_date IS NULL OR (cc.charge_due_date >= $2::date AND cc.charge_due_date <= $3::date))
ORDER BY c.office_id, cc.client_id, cc.charge_due_date NULLS LAST, cc.id
LIMIT $4
