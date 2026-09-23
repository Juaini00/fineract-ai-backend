-- params: {"limit": 50}
-- 50 charge tabungan terbaru (created_on_utc desc, id desc) dalam scope
-- kantor terotorisasi (ditulis independen dari
-- queries/savings/account_charges_recent.sql: konteks charge->akun->klien->
-- kantor lewat CTE charge_context memakai subquery skalar per kolom, bukan
-- empat JOIN berantai + LEFT JOIN LATERAL untuk symbol mata uang).
WITH charge_context AS (
    SELECT
        sac.id AS savings_account_charge_id,
        sac.charge_id,
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
)
SELECT
    cc.client_id,
    (SELECT c.display_name FROM m_client c WHERE c.id = cc.client_id) AS client_display_name,
    (SELECT c.office_id FROM m_client c WHERE c.id = cc.client_id) AS office_id,
    (SELECT o.name FROM m_office o
     WHERE o.id = (SELECT c.office_id FROM m_client c WHERE c.id = cc.client_id)) AS office_name,
    cc.savings_account_id,
    cc.savings_account_charge_id,
    cc.charge_id AS charge_definition_id,
    (SELECT ch.name FROM m_charge ch WHERE ch.id = cc.charge_id) AS charge_name,
    cc.is_penalty,
    cc.charge_time_enum::bigint AS charge_timing_enum,
    cc.currency_code,
    cc.currency_digits::bigint AS currency_digits,
    (SELECT oc.display_symbol FROM m_organisation_currency oc WHERE oc.code = cc.currency_code LIMIT 1) AS currency_display_symbol,
    cc.amount AS amount_due_current,
    coalesce(cc.amount_paid_derived, 0) AS amount_paid,
    coalesce(cc.amount_waived_derived, 0) AS amount_waived,
    coalesce(cc.amount_writtenoff_derived, 0) AS amount_written_off,
    cc.amount_outstanding_derived AS amount_outstanding,
    cc.charge_due_date AS due_date
FROM charge_context cc
WHERE (SELECT c.office_id FROM m_client c WHERE c.id = cc.client_id) = ANY(:'office_ids'::bigint[])
ORDER BY cc.created_on_utc DESC, cc.savings_account_charge_id DESC
LIMIT 50
