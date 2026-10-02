SELECT m.id AS mapping_id,
  g.id AS tax_group_id,
  g.name AS tax_group_name,
  c.id AS component_id,
  c.name AS component_name,
  m.start_date,
  m.end_date
FROM m_tax_group_mappings m
JOIN m_tax_group g ON g.id = m.tax_group_id
JOIN m_tax_component c ON c.id = m.tax_component_id
ORDER BY m.id;
