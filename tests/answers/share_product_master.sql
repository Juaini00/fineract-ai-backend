-- params: {}
-- FIN-109 PROD-3: independent Fineract source comparison.
SELECT q.id product_id, q.name, q.currency_code, q.total_shares, q.issued_shares, q.unit_price, q.start_date, q.end_date
FROM m_share_product AS q
ORDER BY q.id
