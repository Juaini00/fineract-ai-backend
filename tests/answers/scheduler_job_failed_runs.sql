-- params: {"from_date": "2026-04-01", "to_date": "2026-05-31", "limit": 100}
-- FIN-124 / D14: failed scheduler runs in the explicit half-open window
-- [2026-04-01 00:00:00, 2026-06-01 00:00:00) on the stored naive clock. Written
-- independently of queries/scheduler/job_failed_runs.sql: literal timestamp
-- bounds instead of `date + 1`, window functions for each job's latest run
-- instead of a correlated subquery, a different text rendering of the clock.
-- No error_message / error_log column exists in this oracle either.
WITH ranked AS (
    SELECT id, job_id, status, trigger_type, start_time, end_time,
           row_number() OVER (PARTITION BY job_id ORDER BY start_time DESC, id DESC) AS recency
    FROM job_run_history
),
latest AS (
    SELECT job_id, status AS latest_status FROM ranked WHERE recency = 1
)
SELECT
    r.id AS run_id,
    r.job_id,
    j.name AS job_name,
    replace(to_char(r.start_time, 'YYYY-MM-DD HH24:MI:SS.MS'), ' ', 'T') AS start_time,
    replace(to_char(r.end_time, 'YYYY-MM-DD HH24:MI:SS.MS'), ' ', 'T') AS end_time,
    r.status,
    r.trigger_type,
    l.latest_status AS job_latest_run_status
FROM ranked r
JOIN job j ON j.id = r.job_id
JOIN latest l ON l.job_id = r.job_id
WHERE r.status = 'failed'
  AND r.start_time >= TIMESTAMP '2026-04-01 00:00:00'
  AND r.start_time <  TIMESTAMP '2026-06-01 00:00:00'
ORDER BY r.start_time DESC, r.id DESC
LIMIT 100
