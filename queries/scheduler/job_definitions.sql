-- D14: scheduler job definitions (grain: job) with an observation of its runs
-- in an explicit half-open window and its latest run overall.
--   window  = [from_date 00:00:00, to_date + 1 day 00:00:00) on the stored,
--             timezone-naive scheduler clock; no timezone conversion is applied.
--   latest  = newest run by (start_time DESC, id DESC) over the WHOLE history,
--             NOT filtered by the window or by status.
-- A job with no run in the window is reported as `no_observed_run`; that
-- proves only that no run was observed, never success, failure or "never ran".
-- Tenant-wide operational read (owner decision, D14): no office predicate is
-- invented. error_message / error_log are never selected.
SELECT
    j.id AS job_id,
    j.name,
    j.display_name,
    j.cron_expression,
    j.is_active,
    count(w.id) AS runs_in_window,
    count(w.id) FILTER (WHERE w.status = 'success') AS success_runs_in_window,
    count(w.id) FILTER (WHERE w.status = 'failed') AS failed_runs_in_window,
    count(w.id) - count(w.id) FILTER (WHERE w.status IN ('success', 'failed')) AS other_status_runs_in_window,
    CASE WHEN count(w.id) = 0 THEN 'no_observed_run' ELSE 'runs_observed' END AS window_observation,
    lr.id AS latest_run_id,
    to_char(lr.start_time, 'YYYY-MM-DD"T"HH24:MI:SS.MS') AS latest_run_start_time,
    to_char(lr.end_time, 'YYYY-MM-DD"T"HH24:MI:SS.MS') AS latest_run_end_time,
    lr.status AS latest_run_status,
    lr.trigger_type AS latest_run_trigger_type
FROM job j
LEFT JOIN job_run_history w
    ON w.job_id = j.id
   AND w.start_time >= $1::date
   AND w.start_time < $2::date + 1
LEFT JOIN job_run_history lr
    ON lr.id = (
        SELECT h.id FROM job_run_history h
        WHERE h.job_id = j.id
        ORDER BY h.start_time DESC, h.id DESC
        LIMIT 1
    )
GROUP BY j.id, lr.id
ORDER BY j.id
LIMIT $3
