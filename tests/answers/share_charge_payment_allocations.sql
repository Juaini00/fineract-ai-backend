-- params: {"limit": 200}
-- FIN-111 SHARE-5: share charge payment allocations in authorized offices.
-- Written independently of queries/share/share_charge_payment_allocations.sql:
-- client office resolved through account_owner CTE, charge master joined via charge_ref CTE.
WITH account_owner AS (
    SELECT sa.id AS account_id, sa.client_id, c.office_id
    FROM m_share_account sa
    JOIN m_client c ON c.id = sa.client_id
    WHERE c.office_id = ANY(:'office_ids'::bigint[])
),
charge_ref AS (
    SELECT id AS charge_id, currency_code AS charge_currency
    FROM m_charge
)
SELECT
    pb.id AS allocation_id,
    pb.charge_transaction_id AS share_account_charge_id,
    pb.share_transaction_id,
    ao.account_id,
    ao.client_id,
    ao.office_id,
    cr.charge_currency,
    pb.amount AS allocation_amount,
    sac.charge_time_enum::bigint AS charge_time_enum,
    sat.transaction_date,
    sat.type_enum::bigint AS transaction_type_enum,
    sat.is_active
FROM m_share_account_charge_paid_by pb
JOIN m_share_account_charge sac ON sac.id = pb.charge_transaction_id
JOIN account_owner ao ON ao.account_id = sac.account_id
JOIN charge_ref cr ON cr.charge_id = sac.charge_id
LEFT JOIN m_share_account_transactions sat ON sat.id = pb.share_transaction_id
ORDER BY ao.office_id, ao.client_id, pb.id
LIMIT 200
