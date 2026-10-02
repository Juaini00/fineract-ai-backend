-- params: {}
-- FIN-109 PROD-2: a master must include products with no savings account.
SELECT p.id AS product_id
FROM m_savings_product p
WHERE NOT EXISTS (
    SELECT 1 FROM m_savings_account account WHERE account.product_id = p.id
)
ORDER BY p.id
