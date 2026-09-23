-- params: {"charge_name": "Withdrawal fee"}
-- Jumlah charge aktif bertipe "Withdrawal fee" dalam scope kantor
-- terotorisasi (ditulis terpisah dari
-- queries/savings/charge_count_by_type.sql: filter lewat subquery IN,
-- bukan JOIN ke m_client/m_charge).
SELECT count(*)::bigint AS charge_count,
       count(DISTINCT sac.savings_account_id)::bigint AS savings_account_count,
       coalesce(sum(sac.amount_outstanding_derived), 0) AS amount_outstanding_total
FROM m_savings_account_charge sac
WHERE sac.is_active = true
  AND sac.charge_id IN (SELECT id FROM m_charge WHERE lower(name) = lower('Withdrawal fee'))
  AND sac.savings_account_id IN (
      SELECT sa.id FROM m_savings_account sa
      JOIN m_client c ON c.id = sa.client_id
      WHERE c.office_id = ANY(:'office_ids'::bigint[])
  )
