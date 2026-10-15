-- D14 resolver: every registered scheduler job (tenant-level operational
-- data, no office path). The engine pages and searches the result in memory.
SELECT
    j.id AS job_id,
    j.name AS job_name,
    j.is_active
FROM job j
ORDER BY j.id
LIMIT 200
