-- params: {}
-- Full population size for provisioning_run_history without row cap in the default window.
SELECT count(*) AS n
FROM m_provisioning_history
WHERE created_date >= (:'today'::date - interval '12 months')::date
  AND created_date <= :'today'::date
