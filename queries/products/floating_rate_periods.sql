SELECT f.id AS floating_rate_id,
  f.name,
  f.is_base_lending_rate,
  f.is_active,
  p.id AS rate_period_id,
  p.from_date,
  p.interest_rate,
  p.is_differential_to_base_lending_rate,
  p.is_active AS period_active
FROM m_floating_rates f
LEFT JOIN m_floating_rates_periods p ON p.floating_rates_id = f.id
ORDER BY f.id, p.id;
