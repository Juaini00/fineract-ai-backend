-- params: {"transaction_id": "S10476"}
-- Full population size for gl_journal_for_transaction without row cap (one business transaction).
SELECT count(*) AS n
FROM acc_gl_journal_entry gl
WHERE gl.transaction_id = 'S10476'
  AND gl.office_id = ANY(:'office_ids'::bigint[])
