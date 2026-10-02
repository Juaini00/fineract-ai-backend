-- params: {}
-- FIN-109 PROD-2: independent Fineract source comparison.
SELECT q.id product_id, q.name, q.currency_code, q.deposit_type_enum, q.nominal_annual_interest_rate
FROM m_savings_product AS q
ORDER BY q.id
