-- params: {"from_date": ":month_start", "to_date": ":today", "currency_code": null, "product_ids": null, "limit": 10}
-- 10 setoran tabungan terbesar bulan berjalan, office yang diizinkan.
-- client_display_name adalah PII (withheld bila caller tidak can_view_pii);
-- lib/answers.js check() mengabaikan kolom yang ditahan. Ditulis independen
-- dari queries/savings/deposit_top_n.sql (subquery scope kantor, bukan JOIN
-- m_office + ANY langsung).
SELECT t.id                       AS transaction_id,
       t.transaction_date,
       t.amount,
       sa.currency_code,
       t.office_id,
       (SELECT o.name FROM m_office o WHERE o.id = t.office_id)      AS office_name,
       sa.product_id,
       sp.name                    AS product_name,
       sa.client_id,
       c.display_name             AS client_display_name
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
JOIN m_savings_product sp ON sp.id = sa.product_id
LEFT JOIN m_client c ON c.id = sa.client_id
WHERE t.transaction_type_enum = 1
  AND t.is_reversed = false
  AND t.transaction_date >= :'month_start'::date
  AND t.transaction_date <= :'today'::date
  AND t.office_id IN (SELECT o.id FROM m_office o WHERE o.id = ANY(:'office_ids'::bigint[]))
ORDER BY t.amount DESC, t.transaction_date DESC, t.id DESC
LIMIT 10
