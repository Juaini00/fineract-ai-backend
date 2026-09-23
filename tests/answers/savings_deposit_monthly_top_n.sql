-- params: {"from_date": ":today-12m", "to_date": ":today", "currency_code": null, "product_ids": null, "limit": 10}
-- 10 setoran terbesar per bulan kalender selama 12 bulan terakhir. Ditulis
-- independen dari queries/savings/deposit_monthly_top_n.sql: bukan
-- ROW_NUMBER() OVER (PARTITION BY bulan), melainkan daftar bulan yang punya
-- transaksi lalu CROSS JOIN LATERAL yang mengambil top 10 per bulan.
SELECT months.month_start,
       top.transaction_id,
       top.transaction_date,
       top.amount,
       top.currency_code,
       top.office_id,
       top.office_name,
       top.product_id,
       top.product_name,
       top.client_id,
       top.client_display_name
FROM (
    SELECT DISTINCT date_trunc('month', t.transaction_date)::date AS month_start
    FROM m_savings_account_transaction t
    WHERE t.transaction_type_enum = 1
      AND t.is_reversed = false
      AND t.transaction_date >= (:'today'::date - interval '12 months')::date
      AND t.transaction_date <= :'today'::date
      AND t.office_id IN (SELECT o.id FROM m_office o WHERE o.id = ANY(:'office_ids'::bigint[]))
) months
CROSS JOIN LATERAL (
    SELECT t.id            AS transaction_id,
           t.transaction_date,
           t.amount,
           sa.currency_code,
           t.office_id,
           (SELECT o.name FROM m_office o WHERE o.id = t.office_id) AS office_name,
           sa.product_id,
           sp.name         AS product_name,
           sa.client_id,
           c.display_name  AS client_display_name
    FROM m_savings_account_transaction t
    JOIN m_savings_account sa ON sa.id = t.savings_account_id
    JOIN m_savings_product sp ON sp.id = sa.product_id
    LEFT JOIN m_client c ON c.id = sa.client_id
    WHERE t.transaction_type_enum = 1
      AND t.is_reversed = false
      AND t.transaction_date >= (:'today'::date - interval '12 months')::date
      AND t.transaction_date <= :'today'::date
      AND t.office_id IN (SELECT o.id FROM m_office o WHERE o.id = ANY(:'office_ids'::bigint[]))
      AND date_trunc('month', t.transaction_date)::date = months.month_start
    ORDER BY t.amount DESC, t.transaction_date DESC, t.id DESC
    LIMIT 10
) top
ORDER BY months.month_start, top.amount DESC
