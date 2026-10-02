SELECT m.id AS mix_rule_id,
  m.product_id,
  m.restricted_product_id
FROM m_product_mix m
ORDER BY m.id;
