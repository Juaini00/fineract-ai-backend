SELECT p.id AS product_id,
  p.name,
  p.currency_code,
  p.total_shares,
  p.issued_shares,
  p.unit_price,
  p.start_date,
  p.end_date
FROM m_share_product p
ORDER BY p.id;
