-- params: {"limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-130 LINK-6: savings charge settlement evidence in offices 2 and 3.
WITH paying_tx_scope AS (
    SELECT id AS transaction_id, transaction_date, office_id
    FROM m_savings_account_transaction
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
),
account_ref AS (
    SELECT id AS account_id, currency_code
    FROM m_savings_account
)
SELECT
    sacp.id AS charge_paid_by_id,
    sacp.savings_account_charge_id,
    sacp.savings_account_transaction_id,
    sac.savings_account_id,
    sac.charge_id,
    sacp.amount,
    pts.transaction_date,
    ar.currency_code,
    pts.office_id
FROM m_savings_account_charge_paid_by sacp
JOIN m_savings_account_charge sac ON sac.id = sacp.savings_account_charge_id
JOIN paying_tx_scope pts ON pts.transaction_id = sacp.savings_account_transaction_id
JOIN account_ref ar ON ar.account_id = sac.savings_account_id
ORDER BY pts.office_id, pts.transaction_date DESC, sacp.id DESC
LIMIT 200
