-- params: {"search": "Nour Hashem"}
-- Nasabah + rekening tabungan yang dimiliki, dicocokkan lewat nama (LIKE),
-- ditulis dengan subquery berkorelasi untuk hitung akun aktif alih-alih
-- window function seperti queries/client/relationship_lookup.sql.
SELECT
    cl.id AS client_id,
    o.id AS office_id,
    o.name AS office_name,
    (SELECT count(*) FROM m_savings_account s2
      WHERE s2.client_id = cl.id AND s2.status_enum = 300) AS active_savings_account_count,
    sav.id AS savings_account_id,
    CASE WHEN sav.account_no IS NULL THEN NULL ELSE '****' || right(sav.account_no, 4) END AS masked_account_number,
    sav.status_enum AS savings_status_enum,
    sav.currency_code,
    prod.id AS savings_product_id,
    prod.name AS savings_product_name
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
LEFT JOIN m_savings_account sav ON sav.client_id = cl.id
LEFT JOIN m_savings_product prod ON prod.id = sav.product_id
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
  AND cl.display_name ILIKE '%Nour Hashem%'
ORDER BY sav.id NULLS LAST, cl.id
