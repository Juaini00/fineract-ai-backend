-- params: {}
-- FIN-109 PROD-4: independent Fineract source comparison.
SELECT q.id charge_id, q.name, q.currency_code, q.charge_applies_to_enum, q.charge_time_enum, q.charge_calculation_enum, q.amount, q.is_penalty, q.is_active
FROM m_charge AS q
ORDER BY q.id
