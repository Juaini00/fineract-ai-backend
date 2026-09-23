-- params: {"currency_code": null, "product_ids": null}
-- Saldo tabungan aktif (status 300) milik nasabah di office yang diizinkan,
-- satu baris per mata uang (L1.1: tidak pernah dijumlah lintas mata uang).
SELECT sa.currency_code,
       count(*)                                   AS account_count,
       coalesce(sum(sa.account_balance_derived), 0) AS total_balance,
       coalesce(avg(sa.account_balance_derived), 0) AS average_balance,
       coalesce(max(sa.account_balance_derived), 0) AS max_balance
FROM m_savings_account sa
WHERE sa.status_enum = 300
  AND sa.client_id IN (SELECT c.id FROM m_client c WHERE c.office_id = ANY(:'office_ids'::bigint[]))
GROUP BY sa.currency_code
ORDER BY sa.currency_code
