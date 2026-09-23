-- params: {"from_date": ":month_start", "to_date": ":today", "currency_code": null, "product_ids": null, "limit": null}
-- Transaksi tabungan bulan berjalan lintas kantor terotorisasi, tanpa
-- pembatas mata uang/produk/limit (ditulis terpisah dari
-- queries/savings/activity_list.sql: nama produk lewat subquery skalar,
-- bukan JOIN ke m_savings_product; klien tetap LEFT JOIN).
SELECT t.id AS transaction_id,
       t.transaction_date,
       t.transaction_type_enum::bigint AS transaction_type_enum,
       CASE t.transaction_type_enum
           WHEN 1 THEN 'deposit'
           WHEN 2 THEN 'withdrawal'
           WHEN 3 THEN 'interest_posting'
           WHEN 4 THEN 'withdrawal_fee'
           WHEN 5 THEN 'annual_fee'
           WHEN 8 THEN 'dividend_payout'
           WHEN 17 THEN 'withhold_tax'
           WHEN 19 THEN 'escheat'
           WHEN 20 THEN 'amount_hold'
           WHEN 21 THEN 'amount_release'
           ELSE 'other'
       END AS transaction_type,
       t.amount,
       sa.currency_code,
       t.office_id,
       (SELECT o.name FROM m_office o WHERE o.id = t.office_id) AS office_name,
       sa.product_id,
       (SELECT sp.name FROM m_savings_product sp WHERE sp.id = sa.product_id) AS product_name,
       sa.client_id,
       (SELECT c.display_name FROM m_client c WHERE c.id = sa.client_id) AS client_display_name
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
WHERE t.is_reversed = false
  AND t.transaction_date BETWEEN :'month_start'::date AND :'today'::date
  AND t.office_id = ANY(:'office_ids'::bigint[])
ORDER BY t.transaction_date DESC, t.id DESC
