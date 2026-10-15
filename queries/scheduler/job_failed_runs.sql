-- D14: failed scheduler runs (grain: run) whose start_time falls in the
-- explicit half-open window [from_date 00:00:00, to_date + 1 day 00:00:00) on
-- the stored, timezone-naive scheduler clock. A failure stays listed even if a
-- later run of the same job succeeded; the job's latest run overall (not
-- window- or status-filtered) is carried separately in job_latest_run_status.
-- Newest first, ties broken by run id. error_message / error_log are never
-- selected (D14: no raw errors); tenant-wide operational read, no office
-- predicate is invented.
SELECT
    h.id AS run_id,
    h.job_id,
    j.name AS job_name,
    to_char(h.start_time, 'YYYY-MM-DD"T"HH24:MI:SS.MS') AS start_time,
    to_char(h.end_time, 'YYYY-MM-DD"T"HH24:MI:SS.MS') AS end_time,
    h.status,
    h.trigger_type,
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
WHERE h.status = 'failed'
  AND h.start_time >= $1::date
  AND h.start_time < $2::date + 1
ORDER BY h.start_time DESC, h.id DESC
LIMIT $3
