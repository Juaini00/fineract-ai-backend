-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk savings_client_activity klien
-- 65, dipakai lib/answers.js::expectRowCap (FIN-133). Predikat identik
-- dengan oracle utama MINUS LIMIT.
SELECT count(*) AS n
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
WHERE t.is_reversed = false
  AND t.office_id = ANY(:'office_ids'::bigint[])
  AND sa.client_id = 65
