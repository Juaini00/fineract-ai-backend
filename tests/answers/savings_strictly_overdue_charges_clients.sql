-- params: {"as_of_date": ":today", "limit": null}
-- Klien dengan charge tabungan yang jatuh tempo SEBELUM hari ini dan masih
-- belum lunas, dalam scope kantor terotorisasi, tanpa pembatas limit
-- (ditulis terpisah dari queries/savings/strictly_overdue_charges_clients.sql:
-- symbol mata uang lewat subquery skalar, bukan LEFT JOIN LATERAL).
SELECT c.id AS client_id,
       c.display_name AS client_display_name,
       c.office_id,
       o.name AS office_name,
       sa.id AS savings_account_id,
       sac.id AS savings_account_charge_id,
       ch.id AS charge_definition_id,
       ch.name AS charge_name,
       sac.is_penalty,
       sac.charge_time_enum::bigint AS charge_timing_enum,
       sa.currency_code,
       sa.currency_digits::bigint AS currency_digits,
       (SELECT oc.display_symbol FROM m_organisation_currency oc WHERE oc.code = sa.currency_code LIMIT 1) AS currency_display_symbol,
       sac.amount AS amount_due_current,
       coalesce(sac.amount_paid_derived, 0) AS amount_paid,
       coalesce(sac.amount_waived_derived, 0) AS amount_waived,
       coalesce(sac.amount_writtenoff_derived, 0) AS amount_written_off,
       coalesce(sac.amount_paid_derived, 0)
         + coalesce(sac.amount_waived_derived, 0)
         + coalesce(sac.amount_writtenoff_derived, 0)
         + sac.amount_outstanding_derived AS amount_levied_total,
       sac.amount_outstanding_derived AS amount_outstanding,
       sac.charge_due_date AS due_date,
       (:'today'::date - sac.charge_due_date)::bigint AS days_overdue
FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id = sac.savings_account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_office o ON o.id = c.office_id
JOIN m_charge ch ON ch.id = sac.charge_id
WHERE sac.waived = false
  AND sac.is_paid_derived = false
  AND sac.is_active = true
  AND sac.amount_outstanding_derived > 0
  AND sac.charge_due_date < :'today'::date
  AND c.office_id = ANY(:'office_ids'::bigint[])
ORDER BY sac.charge_due_date ASC, sac.amount_outstanding_derived DESC, sac.id
