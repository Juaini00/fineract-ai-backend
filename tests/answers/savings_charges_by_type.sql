-- params: {"charge_name": "Withdrawal fee", "limit": 50}
-- 50 charge "Withdrawal fee" terbaru dalam scope kantor terotorisasi
-- (ditulis independen dari queries/savings/charges_by_type.sql: nama charge
-- disaring dulu lewat CTE matching_charges, konteks akun->klien->kantor
-- lewat subquery skalar, bukan empat JOIN berantai + LEFT JOIN LATERAL
-- untuk symbol mata uang).
WITH matching_charges AS (
    SELECT ch.id AS charge_definition_id, ch.name AS charge_name
    FROM m_charge ch
    WHERE lower(ch.name) = lower('Withdrawal fee')
),
charge_context AS (
    SELECT
        sac.id AS savings_account_charge_id,
        mc.charge_definition_id,
        mc.charge_name,
        sac.savings_account_id,
        sac.is_penalty,
        sac.charge_time_enum,
        sac.amount,
        sac.amount_paid_derived,
        sac.amount_waived_derived,
        sac.amount_writtenoff_derived,
        sac.amount_outstanding_derived,
        sac.charge_due_date,
        sac.created_on_utc,
        (SELECT sa.client_id FROM m_savings_account sa WHERE sa.id = sac.savings_account_id) AS client_id,
        (SELECT sa.currency_code FROM m_savings_account sa WHERE sa.id = sac.savings_account_id) AS currency_code,
        (SELECT sa.currency_digits FROM m_savings_account sa WHERE sa.id = sac.savings_account_id) AS currency_digits
    FROM m_savings_account_charge sac
    JOIN matching_charges mc ON mc.charge_definition_id = sac.charge_id
)
SELECT
    cc.client_id,
    (SELECT c.display_name FROM m_client c WHERE c.id = cc.client_id) AS client_display_name,
    (SELECT c.office_id FROM m_client c WHERE c.id = cc.client_id) AS office_id,
    (SELECT o.name FROM m_office o
     WHERE o.id = (SELECT c.office_id FROM m_client c WHERE c.id = cc.client_id)) AS office_name,
    cc.savings_account_id,
    cc.savings_account_charge_id,
    cc.charge_definition_id,
    cc.charge_name,
    cc.is_penalty,
    cc.charge_time_enum::bigint AS charge_timing_enum,
    cc.currency_code,
    cc.currency_digits::bigint AS currency_digits,
    (SELECT oc.display_symbol FROM m_organisation_currency oc WHERE oc.code = cc.currency_code LIMIT 1) AS currency_display_symbol,
    cc.amount AS amount_due_current,
    coalesce(cc.amount_paid_derived, 0) AS amount_paid,
    coalesce(cc.amount_waived_derived, 0) AS amount_waived,
    coalesce(cc.amount_writtenoff_derived, 0) AS amount_written_off,
    coalesce(cc.amount_paid_derived, 0) + coalesce(cc.amount_waived_derived, 0)
      + coalesce(cc.amount_writtenoff_derived, 0) + cc.amount_outstanding_derived AS amount_levied_total,
    cc.amount_outstanding_derived AS amount_outstanding,
    cc.charge_due_date AS due_date
FROM charge_context cc
WHERE (SELECT c.office_id FROM m_client c WHERE c.id = cc.client_id) = ANY(:'office_ids'::bigint[])
ORDER BY cc.created_on_utc DESC, cc.savings_account_charge_id DESC
LIMIT 50
