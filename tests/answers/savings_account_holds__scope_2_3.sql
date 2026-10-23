-- params: {"from_date": "2026-02-01", "to_date": "2026-09-30", "limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-127 SAV-8: savings account holds and release transactions in offices 2 and 3.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
),
account_ref AS (
    SELECT id AS savings_account_id, client_id, currency_code
    FROM m_savings_account
)
SELECT
    sat.id AS transaction_id,
    sat.savings_account_id,
    ar.client_id,
    cs.office_id,
    sat.office_id AS transaction_office_id,
    sat.transaction_type_enum::bigint AS transaction_type_enum,
    sat.transaction_date,
    sat.amount,
    ar.currency_code,
    sat.hold_type::bigint AS hold_type,
    sat.hold_status::bigint AS hold_status,
    sat.parent_hold_transaction_id,
    sat.release_id_of_hold_amount,
    sat.is_lien_transaction
FROM m_savings_account_transaction sat
JOIN account_ref ar ON ar.savings_account_id = sat.savings_account_id
JOIN client_scope cs ON cs.client_id = ar.client_id
WHERE sat.is_reversed = false
  AND (sat.transaction_type_enum IN (20, 21) OR sat.hold_type IS NOT NULL OR sat.parent_hold_transaction_id IS NOT NULL OR sat.release_id_of_hold_amount IS NOT NULL)
  AND sat.transaction_date >= DATE '2026-02-01' AND sat.transaction_date <= DATE '2026-09-30'
ORDER BY cs.office_id, ar.client_id, sat.transaction_date, sat.id
LIMIT 200
