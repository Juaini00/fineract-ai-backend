-- params: {"from_date": "2025-12-01", "to_date": "2025-12-31", "limit": 100}
-- FIN-124 / D14: job definitions with run observations in the explicit
-- half-open window [2025-12-01 00:00:00, 2026-01-01 00:00:00) on the stored
-- naive clock (runs exist exactly at 2025-12-01 00:00:00 — included — and at
-- 2026-01-01 00:00:00 — excluded). Independent of the production SQL: literal
-- timestamp bounds, DISTINCT ON for the latest run, per-job aggregate CTE.
WITH in_window AS (
    SELECT job_id,
           count(*) AS n,
           sum((status = 'success')::int) AS ok,
           sum((status = 'failed')::int) AS bad
    FROM job_run_history
    WHERE start_time >= TIMESTAMP '2025-12-01 00:00:00'
      AND start_time <  TIMESTAMP '2026-01-01 00:00:00'
    GROUP BY job_id
),
newest AS (
    SELECT DISTINCT ON (job_id) job_id, id, start_time, end_time, status, trigger_type
    FROM job_run_history
    ORDER BY job_id, start_time DESC, id DESC
)
SELECT
    j.id AS job_id,
    j.name,
    j.display_name,
    j.cron_expression,
    j.is_active,
    coalesce(w.n, 0) AS runs_in_window,
    coalesce(w.ok, 0) AS success_runs_in_window,
    coalesce(w.bad, 0) AS failed_runs_in_window,
    coalesce(w.n, 0) - coalesce(w.ok, 0) - coalesce(w.bad, 0) AS other_status_runs_in_window,
    CASE WHEN w.n IS NULL THEN 'no_observed_run' ELSE 'runs_observed' END AS window_observation,
    n.id AS latest_run_id,
    replace(to_char(n.start_time, 'YYYY-MM-DD HH24:MI:SS.MS'), ' ', 'T') AS latest_run_start_time,
    replace(to_char(n.end_time, 'YYYY-MM-DD HH24:MI:SS.MS'), ' ', 'T') AS latest_run_end_time,
    n.status AS latest_run_status,
    n.trigger_type AS latest_run_trigger_type
FROM job j
LEFT JOIN in_window w ON w.job_id = j.id
LEFT JOIN newest n ON n.job_id = j.id
ORDER BY j.id
LIMIT 100
