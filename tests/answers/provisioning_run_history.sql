-- params: {"from_date": ":today-12m", "to_date": ":today", "limit": 200}
-- FIN-116 D07: recorded provisioning run history.
-- Written independently of queries/provisioning/provisioning_run_history.sql:
-- organization-wide reference query, ordered by created_date DESC, id DESC.
SELECT
    id AS history_id,
    created_date,
    createdby_id,
    journal_entry_created,
    lastmodifiedby_id,
    lastmodified_date
FROM m_provisioning_history
WHERE created_date >= (:'today'::date - interval '12 months')::date
  AND created_date <= :'today'::date
ORDER BY created_date DESC, id DESC
LIMIT 200
