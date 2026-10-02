SELECT c.id AS charge_id,
  c.name,
  c.currency_code,
  c.charge_applies_to_enum,
  c.charge_time_enum,
  c.charge_calculation_enum,
  c.amount,
  c.is_penalty,
  c.is_active
FROM m_charge c
ORDER BY c.id;
