-- params: {"job_id": 12, "from_date": "2026-04-01", "to_date": "2026-05-31", "limit": 200}
-- FIN-124 / D14: every run (success AND failed) of job 12 ("Apply penalty to
-- overdue loans") in [2026-04-01 00:00:00, 2026-06-01 00:00:00) on the stored
-- naive clock, oldest first. Independent of queries/scheduler/job_run_history.sql:
-- literal timestamp bounds, window functions for the job's newest run, a
-- different text rendering of the clock. Job 12 failed through 2026-04-09 and
-- succeeded from 2026-04-10 on: both outcomes in one chronological series.
WITH ranked AS (
    SELECT id, job_id, status, trigger_type, start_time, end_time,
           row_number() OVER (PARTITION BY job_id ORDER BY start_time DESC, id DESC) AS recency
    FROM job_run_history
    WHERE job_id = 12
)
SELECT
    r.id AS run_id,
    r.job_id,
    j.name AS job_name,
    replace(to_char(r.start_time, 'YYYY-MM-DD HH24:MI:SS.MS'), ' ', 'T') AS start_time,
    replace(to_char(r.end_time, 'YYYY-MM-DD HH24:MI:SS.MS'), ' ', 'T') AS end_time,
    r.status,
    r.trigger_type,
    (r.recency = 1) AS is_job_latest_run,
    (SELECT x.status FROM ranked x WHERE x.recency = 1) AS job_latest_run_status
FROM ranked r
JOIN job j ON j.id = r.job_id
WHERE r.start_time >= TIMESTAMP '2026-04-01 00:00:00'
  AND r.start_time <  TIMESTAMP '2026-06-01 00:00:00'
ORDER BY r.start_time ASC, r.id ASC
LIMIT 200
