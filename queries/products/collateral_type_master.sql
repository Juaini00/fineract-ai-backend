SELECT c.id AS collateral_type_id,
  c.name,
  c.base_price,
  c.unit_type,
  c.pct_to_base,
  c.currency AS currency_id
FROM m_collateral_management c
ORDER BY c.id;
