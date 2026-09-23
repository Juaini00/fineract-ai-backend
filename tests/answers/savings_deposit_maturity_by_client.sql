-- params: {"client_id": 65}
-- Jadwal jatuh tempo deposito berjangka/berkala milik klien 65, dalam scope
-- kantor terotorisasi. Ditulis independen dari
-- queries/savings/deposit_maturity_by_client.sql: akun disaring dulu lewat
-- CTE eligible_accounts (deposit_type_enum + client_id + scope kantor via
-- EXISTS), BARU dilengkapi detail preclosure/recurring lewat LEFT JOIN —
-- pipeline "filter dulu, lengkapi belakangan", bukan gabungkan semua tabel
-- lalu filter di akhir.
WITH eligible_accounts AS (
    SELECT sa.id AS savings_account_id, sa.client_id, sa.deposit_type_enum, sa.currency_code
    FROM m_savings_account sa
    WHERE sa.deposit_type_enum IN (200, 300)
      AND sa.client_id = 65
      AND EXISTS (
          SELECT 1 FROM m_client c
          WHERE c.id = sa.client_id AND c.office_id = ANY(:'office_ids'::bigint[])
      )
)
SELECT ea.savings_account_id,
       ea.client_id,
       (SELECT c.display_name FROM m_client c WHERE c.id = ea.client_id) AS client_display_name,
       ea.deposit_type_enum::bigint AS deposit_type_enum,
       ea.currency_code,
       t.deposit_amount AS term_deposit_amount,
       t.maturity_amount,
       t.maturity_date,
       t.deposit_period,
       t.deposit_period_frequency_enum::bigint AS deposit_period_frequency_enum,
       r.mandatory_recommended_deposit_amount,
       r.is_mandatory,
       r.total_overdue_amount,
       r.no_of_overdue_installments
FROM eligible_accounts ea
LEFT JOIN m_deposit_account_term_and_preclosure t ON t.savings_account_id = ea.savings_account_id
LEFT JOIN m_deposit_account_recurring_detail r ON r.savings_account_id = ea.savings_account_id
ORDER BY ea.savings_account_id
LIMIT 100
