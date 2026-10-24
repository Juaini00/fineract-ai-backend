-- params: {"gl_code": "20101"}
-- Full population size for gl_journal_for_account without row cap (named GL account).
SELECT count(*) AS n
FROM acc_gl_journal_entry gl
JOIN acc_gl_account a ON a.id = gl.account_id
WHERE a.gl_code = '20101'
  AND gl.office_id = ANY(:'office_ids'::bigint[])
