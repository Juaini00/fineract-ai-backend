-- D14 freshness probe (XR-ASOF): batch evidence for one named job, read with
-- the Fineract business dates. This is run-level evidence ONLY: a successful
-- run never proves business-period or account coverage (that is a separate,
-- account-level probe). One row; job columns are NULL when the job name is
-- not registered. error_message / error_log are never selected.
SELECT
    (SELECT b.date FROM m_business_date b WHERE b.type = 'COB_DATE') AS cob_date,
    (SELECT b.date FROM m_business_date b WHERE b.type = 'BUSINESS_DATE') AS business_date,
    j.id AS job_id,
    j.is_active AS job_is_active,
    lr.id AS latest_run_id,
    to_char(lr.start_time, 'YYYY-MM-DD"T"HH24:MI:SS.MS') AS latest_run_start_time,
    lr.status AS latest_run_status,
    ls.id AS latest_success_run_id,
    to_char(ls.start_time, 'YYYY-MM-DD"T"HH24:MI:SS.MS') AS latest_success_start_time
FROM job j
LEFT JOIN job_run_history lr
    ON lr.id = (
        SELECT a.id FROM job_run_history a
        WHERE a.job_id = j.id
        ORDER BY a.start_time DESC, a.id DESC
        LIMIT 1
    )
LEFT JOIN job_run_history ls
    ON ls.id = (
        SELECT s.id FROM job_run_history s
        WHERE s.job_id = j.id AND s.status = 'success'
        ORDER BY s.start_time DESC, s.id DESC
        LIMIT 1
    )
WHERE j.name = $1
