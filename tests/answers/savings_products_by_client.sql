-- params: {"client_id": 65}
-- Produk tabungan berbeda yang dimiliki klien 65, dalam scope kantor
-- terotorisasi (ditulis terpisah dari
-- queries/savings/products_by_client.sql: scope kantor lewat EXISTS, bukan
-- JOIN ke m_client; ORDER BY id ditambahkan supaya determinstik).
SELECT DISTINCT sp.id AS savings_product_id,
       sp.name AS savings_product_name,
       sa.client_id,
       sp.currency_code,
       sp.deposit_type_enum::bigint AS deposit_type_enum,
       sp.nominal_annual_interest_rate
FROM m_savings_account sa
JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE sa.client_id = 65
  AND EXISTS (
      SELECT 1 FROM m_client c
      WHERE c.id = sa.client_id AND c.office_id = ANY(:'office_ids'::bigint[])
  )
ORDER BY sp.id
