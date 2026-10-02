SELECT p.id AS product_id,
  p.name,
  p.currency_code,
  p.deposit_type_enum,
  p.nominal_annual_interest_rate
FROM m_savings_product p
ORDER BY p.id;
