-- params: {}
-- FIN-109 PROD-5: independent Fineract source comparison.
SELECT q.id collateral_type_id, q.name, q.base_price, q.unit_type, q.pct_to_base, q.currency currency_id
FROM m_collateral_management AS q
ORDER BY q.id
