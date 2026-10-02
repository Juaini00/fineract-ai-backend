-- params: {}
-- FIN-109 PROD-10: independent Fineract source comparison.
SELECT link.id mapping_id, tax_group.id tax_group_id, tax_group.name tax_group_name, component.id component_id, component.name component_name, link.start_date, link.end_date
FROM m_tax_group tax_group
JOIN m_tax_group_mappings link ON link.tax_group_id=tax_group.id
JOIN m_tax_component component ON component.id=link.tax_component_id
ORDER BY link.id
