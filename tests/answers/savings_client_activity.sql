-- params: {"client_id": 65, "limit": null}
-- Seluruh transaksi tabungan (tidak dibalik) milik klien 65 (Jasmin Dao,
-- klien dengan transaksi terbanyak), tanpa pembatas limit (ditulis terpisah
-- dari queries/savings/activity_by_client.sql: filter klien lewat EXISTS,
-- bukan JOIN ke m_client).
SELECT t.id AS savings_transaction_id,
       t.savings_account_id,
       sa.client_id,
       (SELECT c.display_name FROM m_client c WHERE c.id = sa.client_id) AS client_display_name,
       t.transaction_type_enum::bigint AS transaction_type_enum,
       t.transaction_date,
       t.amount,
       t.running_balance_derived AS running_balance
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
WHERE t.is_reversed = false
  AND t.office_id = ANY(:'office_ids'::bigint[])
  AND sa.client_id = 65
ORDER BY t.transaction_date DESC, t.id DESC
