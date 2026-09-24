-- params: {"from_date": ":month_start", "to_date": ":today", "currency_code": null, "product_ids": null}
-- Total setoran tabungan (transaction_type_enum=1, tidak dibalik) bulan
-- berjalan per mata uang (L1.1: tidak pernah dijumlah lintas mata uang),
-- dibatasi pada office yang diizinkan. Ditulis independen dari
-- queries/savings/deposit_total.sql (scope kantor lewat subquery m_office,
-- bukan JOIN + ANY langsung).
SELECT :'month_start'::date                    AS from_date,
       :'today'::date                          AS to_date,
       sa.currency_code,
       coalesce(sum(t.amount), 0)              AS total_deposit_amount,
       count(*)                                AS deposit_count
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
WHERE t.transaction_type_enum = 1
  AND t.is_reversed = false
  AND t.transaction_date >= :'month_start'::date
  AND t.transaction_date <= :'today'::date
  AND t.office_id IN (SELECT o.id FROM m_office o WHERE o.id = ANY(:'office_ids'::bigint[]))
GROUP BY sa.currency_code
ORDER BY sa.currency_code
