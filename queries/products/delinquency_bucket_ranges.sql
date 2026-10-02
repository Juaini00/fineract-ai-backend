SELECT b.id AS bucket_id,
  b.name AS bucket_name,
  m.id AS range_mapping_id,
  r.id AS range_id,
  r.classification,
  r.min_age_days,
  r.max_age_days
FROM m_delinquency_bucket b
LEFT JOIN m_delinquency_bucket_mappings m ON m.delinquency_bucket_id = b.id
LEFT JOIN m_delinquency_range r ON r.id = m.delinquency_range_id
ORDER BY b.id, m.id;
