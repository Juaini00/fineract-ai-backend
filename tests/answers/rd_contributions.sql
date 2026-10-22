-- params: {"from_date": "2026-01-01", "to_date": "2026-03-31", "limit": 200}
-- FDRD-8 / FDRD-9: recurring deposit contributions for authorized offices.
-- Expected config and actual recorded deposits are kept separate.
WITH client_owner AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
)
SELECT sa.id AS savings_account_id,
       sa.client_id,
       co.office_id,
       sa.currency_code,
       rd.mandatory_recommended_deposit_amount AS expected_deposit_amount,
       COALESCE((
           SELECT SUM(sat.amount)
           FROM m_savings_account_transaction sat
           WHERE sat.savings_account_id = sa.id
             AND sat.transaction_type_enum = 1
             AND NOT sat.is_reversed
             AND (sat.transaction_date >= DATE '2026-01-01' AND sat.transaction_date <= DATE '2026-03-31')
       ), 0.0) AS actual_deposit_amount,
       COALESCE((
           SELECT COUNT(*)
           FROM m_savings_account_transaction sat
           WHERE sat.savings_account_id = sa.id
             AND sat.transaction_type_enum = 1
             AND NOT sat.is_reversed
             AND (sat.transaction_date >= DATE '2026-01-01' AND sat.transaction_date <= DATE '2026-03-31')
       ), 0) AS actual_deposit_count,
       dat.deposit_period,
       dat.deposit_period_frequency_enum::bigint AS deposit_period_frequency_enum,
       rd.is_calendar_inherited,
       rd.is_mandatory
FROM m_savings_account sa
JOIN client_owner co ON co.client_id = sa.client_id
JOIN m_deposit_account_recurring_detail rd ON rd.savings_account_id = sa.id
LEFT JOIN m_deposit_account_term_and_preclosure dat ON dat.savings_account_id = sa.id
WHERE sa.deposit_type_enum = 300
  AND sa.submittedon_date <= DATE '2026-03-31'
  AND (sa.closedon_date IS NULL OR sa.closedon_date >= DATE '2026-01-01')
ORDER BY co.office_id, sa.client_id, sa.id
LIMIT 200
