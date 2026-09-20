SELECT DISTINCT
    ch.id AS charge_definition_id,
    ch.name AS charge_name,
    ch.currency_code,
    ch.is_penalty
FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id = sac.savings_account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_charge ch ON ch.id = sac.charge_id
WHERE c.office_id = ANY($1::bigint[])
  AND ch.is_active = true
  AND ch.is_deleted = false
