-- params: {}
-- FIN-109 PROD-11: independent Fineract source comparison.
SELECT rate.id floating_rate_id, rate.name, rate.is_base_lending_rate, rate.is_active, period.id rate_period_id, period.from_date, period.interest_rate, period.is_differential_to_base_lending_rate, period.is_active period_active
FROM m_floating_rates rate
LEFT
JOIN m_floating_rates_periods period ON period.floating_rates_id=rate.id
ORDER BY rate.id,period.id
