SELECT p.id AS product_id,
  p.name,
  p.currency_code,
  p.principal_amount,
  p.min_principal_amount,
  p.max_principal_amount,
  p.nominal_interest_rate_per_period,
  p.number_of_repayments
FROM m_product_loan p
ORDER BY p.id;
