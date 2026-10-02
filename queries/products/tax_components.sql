SELECT c.id AS component_id,
  c.name,
  c.percentage,
  c.start_date
FROM m_tax_component c
ORDER BY c.id;
