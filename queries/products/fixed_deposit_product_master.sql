SELECT p.id AS product_id,
  p.name,
  p.currency_code,
  t.min_deposit_term,
  t.max_deposit_term,
  t.deposit_amount,
  t.pre_closure_penal_applicable,
  t.pre_closure_penal_interest
FROM m_savings_product p
JOIN m_deposit_product_term_and_preclosure t ON t.savings_product_id = p.id
WHERE p.deposit_type_enum = 200
ORDER BY p.id;
