-- params: {}
-- FIN-109 PROD-6: independent Fineract source comparison.
SELECT bucket.id bucket_id, bucket.name bucket_name, bridge.id range_mapping_id, age.id range_id, age.classification, age.min_age_days, age.max_age_days
FROM m_delinquency_bucket bucket
LEFT
JOIN m_delinquency_bucket_mappings bridge ON bucket.id=bridge.delinquency_bucket_id
LEFT
JOIN m_delinquency_range age ON bridge.delinquency_range_id=age.id
ORDER BY bucket.id,bridge.id
