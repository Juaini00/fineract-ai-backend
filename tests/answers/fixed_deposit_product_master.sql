-- params: {}
-- FIN-109 PROD-8: independent Fineract source comparison.
SELECT product.id product_id, product.name, product.currency_code, terms.min_deposit_term, terms.max_deposit_term, terms.deposit_amount, terms.pre_closure_penal_applicable, terms.pre_closure_penal_interest
FROM m_deposit_product_term_and_preclosure terms
JOIN m_savings_product product ON terms.savings_product_id=product.id
WHERE product.deposit_type_enum=200
ORDER BY product.id
