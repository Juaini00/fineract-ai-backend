-- D14: every run of ONE selected job (success AND failed attempts) whose
-- start_time falls in the explicit half-open window
-- [from_date 00:00:00, to_date + 1 day 00:00:00) on the stored, timezone-naive
-- scheduler clock. Chronological: oldest first, ties by run id. The job's
-- latest run overall (not window-filtered) is carried per row so earlier
-- failures can be read against what happened later. error_message / error_log
-- are never selected; tenant-wide operational read, no office predicate.
SELECT
    h.id AS run_id,
    h.job_id,
    j.name AS job_name,
    to_char(h.start_time, 'YYYY-MM-DD"T"HH24:MI:SS.MS') AS start_time,
    to_char(h.end_time, 'YYYY-MM-DD"T"HH24:MI:SS.MS') AS end_time,
    h.status,
    h.trigger_type,
    (h.id = lr.id) AS is_job_latest_run,
    lr.status AS job_latest_run_status
FROM job_run_history h
JOIN job j ON j.id = h.job_id
LEFT JOIN job_run_history lr
    ON lr.id = (
        SELECT l.id FROM job_run_history l
        WHERE l.job_id = h.job_id
        ORDER BY l.start_time DESC, l.id DESC
        LIMIT 1
    )
WHERE h.job_id = $1
  AND h.start_time >= $2::date
  AND h.start_time < $3::date + 1
ORDER BY h.start_time ASC, h.id ASC
LIMIT $4
