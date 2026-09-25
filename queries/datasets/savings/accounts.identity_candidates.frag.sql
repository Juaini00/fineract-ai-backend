SELECT
    savings_account_id,
    masked_account_number,
    office_id,
    office_name,
    savings_product_name
FROM base
ORDER BY savings_account_id
LIMIT 25
