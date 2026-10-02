SELECT p.id AS product_id,
  p.name,
  p.currency_code,
  t.deposit_amount,
  r.is_mandatory,
  r.allow_withdrawal,
  r.adjust_advance_towards_future_payments
FROM m_savings_product p
JOIN m_deposit_product_term_and_preclosure t ON t.savings_product_id = p.id
JOIN m_deposit_product_recurring_detail r ON r.savings_product_id = p.id
WHERE p.deposit_type_enum = 300
ORDER BY p.id;
