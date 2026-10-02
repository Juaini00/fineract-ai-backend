-- params: {}
-- FIN-109 PROD-7: independent Fineract source comparison.
SELECT pair.id mix_rule_id, pair.product_id, pair.restricted_product_id
FROM m_product_mix pair
ORDER BY pair.id
