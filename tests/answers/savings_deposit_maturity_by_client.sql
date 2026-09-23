-- params: {"client_id": 65}
-- Jadwal jatuh tempo deposito berjangka/berkala milik klien 65, dalam scope
-- kantor terotorisasi (ditulis terpisah dari
-- queries/savings/deposit_maturity_by_client.sql: filter klien lewat
-- WHERE langsung pada sa.client_id, bukan JOIN ke m_client untuk predikat).
SELECT sa.id AS savings_account_id,
       sa.client_id,
       (SELECT c.display_name FROM m_client c WHERE c.id = sa.client_id) AS client_display_name,
       sa.deposit_type_enum::bigint AS deposit_type_enum,
       sa.currency_code,
       t.deposit_amount AS term_deposit_amount,
       t.maturity_amount,
       t.maturity_date,
       t.deposit_period,
       t.deposit_period_frequency_enum::bigint AS deposit_period_frequency_enum,
       r.mandatory_recommended_deposit_amount,
       r.is_mandatory,
       r.total_overdue_amount,
       r.no_of_overdue_installments
FROM m_savings_account sa
LEFT JOIN m_deposit_account_term_and_preclosure t ON t.savings_account_id = sa.id
LEFT JOIN m_deposit_account_recurring_detail r ON r.savings_account_id = sa.id
WHERE sa.deposit_type_enum IN (200, 300)
  AND sa.client_id = 65
  AND EXISTS (
      SELECT 1 FROM m_client c
      WHERE c.id = sa.client_id AND c.office_id = ANY(:'office_ids'::bigint[])
  )
ORDER BY sa.id
LIMIT 100
