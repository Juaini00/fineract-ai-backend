-- params: {}
-- FIN-109 PROD-10: independent Fineract source comparison.
SELECT component.id component_id, component.name, component.percentage, component.start_date
FROM m_tax_component component
ORDER BY component.id
