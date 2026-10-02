SELECT p.id AS product_id,
  p.name AS product_name,
  b.id AS bucket_id,
  b.name AS bucket_name
FROM m_product_loan p
JOIN m_delinquency_bucket b ON b.id = p.delinquency_bucket_id
ORDER BY p.id;
