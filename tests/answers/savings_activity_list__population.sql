-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk savings_activity_list, dipakai
-- lib/answers.js::expectRowCap (FIN-133) untuk menentukan apakah row cap 100
-- benar-benar terpotong bulan ini. Predikat identik dengan oracle utama
-- MINUS LIMIT.
SELECT count(*) AS n
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
WHERE t.is_reversed = false
  AND t.transaction_date BETWEEN :'month_start'::date AND :'today'::date
  AND t.office_id = ANY(:'office_ids'::bigint[])
