-- params: {}
-- FIN-124 / D14: independent two-number proof for the loan_arrears freshness
-- hand-off. Batch evidence (Loan COB) and account-level coverage are computed
-- here without the production probes: window function for the newest runs, an
-- explicit scope CTE, and `NOT (last_closed >= cob)` instead of `< cob`.
WITH cob AS (
    SELECT max(date) FILTER (WHERE type = 'COB_DATE') AS cob_date,
           max(date) FILTER (WHERE type = 'BUSINESS_DATE') AS business_date
    FROM m_business_date
),
runs AS (
    SELECT h.id, h.status, h.start_time,
           row_number() OVER (ORDER BY h.start_time DESC, h.id DESC) AS recency,
           row_number() OVER (PARTITION BY (h.status = 'success') ORDER BY h.start_time DESC, h.id DESC) AS recency_in_status
    FROM job_run_history h JOIN job j ON j.id = h.job_id
    WHERE j.name = 'Loan COB'
),
scoped AS (
    SELECT l.id, l.last_closed_business_date
    FROM m_loan l JOIN m_client c ON c.id = l.client_id
    WHERE c.office_id = ANY(:'office_ids'::bigint[]) AND l.loan_status_id = ANY(ARRAY[300, 303, 304])
    UNION
    SELECT l.id, l.last_closed_business_date
    FROM m_loan l JOIN m_group g ON g.id = l.group_id
    WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[]) AND l.loan_status_id = ANY(ARRAY[300, 303, 304])
)
SELECT
    cob.cob_date,
    cob.business_date,
    (SELECT status FROM runs WHERE recency = 1) AS latest_run_status,
    (SELECT replace(to_char(start_time, 'YYYY-MM-DD HH24:MI:SS.MS'), ' ', 'T') FROM runs WHERE recency = 1) AS latest_run_start_time,
    (SELECT replace(to_char(start_time, 'YYYY-MM-DD HH24:MI:SS.MS'), ' ', 'T') FROM runs WHERE status = 'success' AND recency_in_status = 1) AS latest_success_start_time,
    (SELECT count(*) FROM scoped) AS in_scope_loans,
    (SELECT count(*) FROM scoped s WHERE NOT (s.last_closed_business_date >= cob.cob_date) OR s.last_closed_business_date IS NULL) AS loans_behind_cob,
    (SELECT min(last_closed_business_date) FROM scoped) AS oldest_last_closed_business_date
FROM cob
