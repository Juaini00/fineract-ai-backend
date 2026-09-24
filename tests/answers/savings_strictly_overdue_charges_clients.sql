-- params: {"as_of_date": ":today", "limit": 10000}
-- Klien dengan charge tabungan yang jatuh tempo SEBELUM hari ini dan masih
-- belum lunas, dalam scope kantor terotorisasi, dibatasi row cap 10000
-- (savings/strictly_overdue_charges_clients.yaml hard_cap=10000, FIN-133;
-- lihat savings_strictly_overdue_charges_clients__population.sql). Ditulis
-- independen dari queries/savings/strictly_overdue_charges_clients.sql:
-- konteks charge->akun->klien->kantor lewat CTE charge_context memakai
-- subquery skalar per kolom, bukan lima JOIN berantai + LEFT JOIN LATERAL
-- untuk symbol mata uang.
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
        (SELECT sa.client_id FROM m_savings_account sa WHERE sa.id = sac.savings_account_id) AS client_id,
        (SELECT sa.currency_code FROM m_savings_account sa WHERE sa.id = sac.savings_account_id) AS currency_code,
        (SELECT sa.currency_digits FROM m_savings_account sa WHERE sa.id = sac.savings_account_id) AS currency_digits
    FROM m_savings_account_charge sac
    WHERE sac.waived = false
      AND sac.is_paid_derived = false
      AND sac.is_active = true
      AND sac.amount_outstanding_derived > 0
      AND sac.charge_due_date < :'today'::date
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
    coalesce(cc.amount_paid_derived, 0) + coalesce(cc.amount_waived_derived, 0)
      + coalesce(cc.amount_writtenoff_derived, 0) + cc.amount_outstanding_derived AS amount_levied_total,
    cc.amount_outstanding_derived AS amount_outstanding,
    cc.charge_due_date AS due_date,
    (:'today'::date - cc.charge_due_date)::bigint AS days_overdue
FROM charge_context cc
WHERE (SELECT c.office_id FROM m_client c WHERE c.id = cc.client_id) = ANY(:'office_ids'::bigint[])
ORDER BY cc.charge_due_date ASC, cc.amount_outstanding_derived DESC, cc.savings_account_charge_id
LIMIT 10000
