-- params: {"from_date": ":month_start", "to_date": ":today", "currency_code": null, "product_ids": null}
-- Total penarikan tabungan (transaction_type_enum=2, tidak dibalik) bulan
-- berjalan per mata uang, dibatasi pada office yang diizinkan. Ditulis
-- independen dari queries/savings/withdrawal_total.sql.
SELECT :'month_start'::date                    AS from_date,
       :'today'::date                          AS to_date,
       sa.currency_code,
       coalesce(sum(t.amount), 0)              AS total_withdrawal_amount,
       count(*)                                AS withdrawal_count
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
WHERE t.transaction_type_enum = 2
  AND t.is_reversed = false
  AND t.transaction_date >= :'month_start'::date
  AND t.transaction_date <= :'today'::date
  AND t.office_id IN (SELECT o.id FROM m_office o WHERE o.id = ANY(:'office_ids'::bigint[]))
GROUP BY sa.currency_code
ORDER BY sa.currency_code
