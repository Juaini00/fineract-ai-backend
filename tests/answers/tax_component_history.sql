-- params: {}
-- FIN-109 PROD-10: independent Fineract source comparison.
SELECT rate.id history_id, component.id component_id, component.name component_name, rate.percentage, rate.start_date, rate.end_date
FROM m_tax_component component
JOIN m_tax_component_history rate ON rate.tax_component_id=component.id
ORDER BY rate.id
