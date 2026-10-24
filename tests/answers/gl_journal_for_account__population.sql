-- params: {}
-- Full population size for gl_journal_for_account without row cap.
SELECT count(*) AS n
FROM acc_gl_journal_entry gl
WHERE gl.office_id = ANY(:'office_ids'::bigint[])
