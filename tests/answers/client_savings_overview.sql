-- params: {"search": "Jasmin Dao", "limit": 10}
-- Ringkasan tabungan satu nasabah: jumlah rekening, biaya belum terbayar
-- aktif, dan jumlah transaksi. Ditulis dengan LATERAL terpisah per metrik,
-- bukan tiga LEFT JOIN LATERAL seperti queries/client/savings_overview.sql.
SELECT
    cl.id AS client_id,
    o.id AS office_id,
    o.name AS office_name,
    acc.total_cnt AS savings_account_count,
    acc.active_cnt AS active_savings_account_count,
    chg.unpaid_cnt AS active_unpaid_charge_count,
    chg.unpaid_amt AS active_unpaid_charge_amount_outstanding,
    txn.txn_cnt AS transaction_count
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
CROSS JOIN LATERAL (
    SELECT count(*) AS total_cnt, count(*) FILTER (WHERE s.status_enum = 300) AS active_cnt
    FROM m_savings_account s WHERE s.client_id = cl.id
) acc
CROSS JOIN LATERAL (
    SELECT count(*) AS unpaid_cnt, coalesce(sum(sc.amount_outstanding_derived), 0) AS unpaid_amt
    FROM m_savings_account_charge sc
    JOIN m_savings_account s ON s.id = sc.savings_account_id
    WHERE s.client_id = cl.id AND sc.is_active = true AND sc.waived = false
      AND sc.is_paid_derived = false AND sc.amount_outstanding_derived > 0
) chg
CROSS JOIN LATERAL (
    SELECT count(*) AS txn_cnt
    FROM m_savings_account_transaction t
    JOIN m_savings_account s ON s.id = t.savings_account_id
    WHERE s.client_id = cl.id AND t.is_reversed = false
) txn
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
  AND cl.display_name ILIKE '%Jasmin Dao%'
ORDER BY cl.display_name, cl.id
LIMIT 10
