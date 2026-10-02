SELECT h.id AS history_id,
  c.id AS component_id,
  c.name AS component_name,
  h.percentage,
  h.start_date,
  h.end_date
FROM m_tax_component_history h
JOIN m_tax_component c ON c.id = h.tax_component_id
ORDER BY h.id;
