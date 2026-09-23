-- params: {"from_date": ":today-12m", "to_date": ":today", "currency_code": null, "product_ids": null}
-- Total setoran per bulan kalender selama 12 bulan terakhir per mata uang.
-- from_date dihitung dari :'today' (business_today - 12m planner, day
-- preserving, dijepit akhir bulan) — sama seperti token header, dihitung di
-- SQL supaya tidak pernah basi. Ditulis independen dari
-- queries/savings/deposit_monthly_breakdown.sql.
SELECT date_trunc('month', t.transaction_date)::date AS month_start,
       sa.currency_code,
       coalesce(sum(t.amount), 0)                    AS total_deposit_amount,
       count(*)                                      AS deposit_count
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
WHERE t.transaction_type_enum = 1
  AND t.is_reversed = false
  AND t.transaction_date >= (:'today'::date - interval '12 months')::date
  AND t.transaction_date <= :'today'::date
  AND t.office_id IN (SELECT o.id FROM m_office o WHERE o.id = ANY(:'office_ids'::bigint[]))
GROUP BY date_trunc('month', t.transaction_date), sa.currency_code
ORDER BY month_start, sa.currency_code
