-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk savings_pending_charges_clients,
-- dipakai lib/answers.js::expectRowCap (FIN-133). Predikat identik dengan
-- oracle utama MINUS LIMIT (correlated subquery client_id/office_id
-- diulang di sini karena grain populasi adalah baris charge, sama seperti
-- oracle utama, bukan klien).
SELECT count(*) AS n
FROM m_savings_account_charge sac
WHERE sac.waived = false
  AND sac.is_paid_derived = false
  AND sac.is_active = true
  AND sac.amount_outstanding_derived > 0
  AND (SELECT c.office_id FROM m_client c
       WHERE c.id = (SELECT sa.client_id FROM m_savings_account sa WHERE sa.id = sac.savings_account_id))
      = ANY(:'office_ids'::bigint[])
