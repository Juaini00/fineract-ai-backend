-- params: {}
-- FIN-109 PROD-9: independent Fineract source comparison.
SELECT product.id product_id, product.name, product.currency_code, terms.deposit_amount, recurring.is_mandatory, recurring.allow_withdrawal, recurring.adjust_advance_towards_future_payments
FROM m_deposit_product_recurring_detail recurring
JOIN m_savings_product product ON product.id=recurring.savings_product_id
JOIN m_deposit_product_term_and_preclosure terms ON terms.savings_product_id=product.id
WHERE product.deposit_type_enum=300
ORDER BY product.id
