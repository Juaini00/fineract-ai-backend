-- params: {"limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-111 SHARE-5: share account charges in offices 2 and 3.
WITH account_owner AS (
    SELECT sa.id AS account_id, sa.client_id, c.office_id
    FROM m_share_account sa
    JOIN m_client c ON c.id = sa.client_id
    WHERE c.office_id = ANY(ARRAY[2,3]::bigint[])
),
charge_ref AS (
    SELECT id AS charge_id, name AS charge_name, currency_code AS charge_currency
    FROM m_charge
)
SELECT
    sac.id AS share_account_charge_id,
    sac.account_id,
    ao.client_id,
    ao.office_id,
    sac.charge_id,
    cr.charge_name,
    cr.charge_currency,
    sac.charge_time_enum::bigint AS charge_time_enum,
    sac.charge_calculation_enum::bigint AS charge_calculation_enum,
    sac.charge_payment_mode_enum::bigint AS charge_payment_mode_enum,
    sac.amount,
    sac.amount_paid_derived,
    sac.amount_waived_derived,
    sac.amount_writtenoff_derived,
    sac.amount_outstanding_derived,
    sac.is_paid_derived,
    sac.waived,
    sac.is_active
FROM m_share_account_charge sac
JOIN account_owner ao ON ao.account_id = sac.account_id
JOIN charge_ref cr ON cr.charge_id = sac.charge_id
ORDER BY ao.office_id, ao.client_id, sac.id
LIMIT 200
