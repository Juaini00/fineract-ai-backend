-- params: {"client_id": 65}
-- Produk tabungan berbeda yang dimiliki klien 65, dalam scope kantor
-- terotorisasi. Ditulis independen dari
-- queries/savings/products_by_client.sql (yang TIDAK punya ORDER BY sama
-- sekali — urutan job tidak terjamin, lihat lib/answers.js::sortRows dan
-- panggilan check() dengan options.sortBy=["savings_product_id"] di
-- products-by-client-answer.yml): id produk di-DISTINCT dulu lewat CTE
-- client_product_ids, BARU di-JOIN ke m_savings_product — bukan JOIN produk
-- langsung lalu SELECT DISTINCT di atasnya.
WITH client_accounts AS (
    SELECT sa.product_id
    FROM m_savings_account sa
    WHERE sa.client_id = 65
      AND EXISTS (
          SELECT 1 FROM m_client c
          WHERE c.id = sa.client_id AND c.office_id = ANY(:'office_ids'::bigint[])
      )
),
client_product_ids AS (
    SELECT DISTINCT product_id FROM client_accounts
)
SELECT sp.id AS savings_product_id,
       sp.name AS savings_product_name,
       65::bigint AS client_id,
       sp.currency_code,
       sp.deposit_type_enum::bigint AS deposit_type_enum,
       sp.nominal_annual_interest_rate
FROM client_product_ids cpi
JOIN m_savings_product sp ON sp.id = cpi.product_id
ORDER BY sp.id
