-- params: {}
-- FIN-109 PROD-6: independent Fineract source comparison.
SELECT loan_product.id product_id, loan_product.name product_name, bucket.id bucket_id, bucket.name bucket_name
FROM m_delinquency_bucket bucket
JOIN m_product_loan loan_product ON loan_product.delinquency_bucket_id=bucket.id
ORDER BY loan_product.id
