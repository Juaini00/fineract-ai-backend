-- params: {}
-- FIN-109 PROD-1: independent Fineract source comparison.
SELECT q.id product_id, q.name, q.currency_code, q.principal_amount, q.min_principal_amount, q.max_principal_amount, q.nominal_interest_rate_per_period, q.number_of_repayments
FROM m_product_loan AS q
ORDER BY q.id
