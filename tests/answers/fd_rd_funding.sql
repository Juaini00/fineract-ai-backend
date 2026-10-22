-- params: {"from_date": "2026-01-01", "to_date": "2026-03-31", "limit": 200}
-- FDRD-3: recorded deposit transactions near submission/activation for authorized offices.
-- Funding is a reading of initial deposits near activation, not an event type.
WITH client_owner AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
)
SELECT sat.id AS transaction_id,
       sa.id AS savings_account_id,
       sa.client_id,
       co.office_id,
       sa.deposit_type_enum::bigint AS deposit_type_enum,
       sa.currency_code,
       sat.transaction_date,
       sat.amount,
       sa.submittedon_date,
       sa.activatedon_date,
       sat.is_reversed
FROM m_savings_account_transaction sat
JOIN m_savings_account sa ON sat.savings_account_id = sa.id
JOIN client_owner co ON co.client_id = sa.client_id
WHERE sa.deposit_type_enum IN (200, 300)
  AND sat.transaction_type_enum = 1
  AND NOT sat.is_reversed
  AND (sat.transaction_date >= DATE '2026-01-01' AND sat.transaction_date <= DATE '2026-03-31')
  AND sat.transaction_date >= sa.submittedon_date
  AND (sa.activatedon_date IS NULL OR sat.transaction_date <= sa.activatedon_date + INTERVAL '30 days')
ORDER BY co.office_id, sa.client_id, sat.transaction_date, sat.id
LIMIT 200
