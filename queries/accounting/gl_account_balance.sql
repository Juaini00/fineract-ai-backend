-- D12: deterministic balance and turnover aggregation for one named GL account.
-- Reversal exclusion (§31 anti-double-count): excludes reversed entries and paired reversal entries from sums.
-- Office scope is the journal entry office bound inside SQL.
-- Grouped per office and currency without cross-currency consolidation.
SELECT
    gl.office_id,
    gl.currency_code,
    COALESCE(SUM(CASE WHEN gl.type_enum = 2 AND NOT gl.reversed AND NOT EXISTS (SELECT 1 FROM acc_gl_journal_entry rev WHERE rev.reversal_id = gl.id) THEN gl.amount ELSE 0 END), 0)::numeric(19,6) AS debit_total,
    COALESCE(SUM(CASE WHEN gl.type_enum = 1 AND NOT gl.reversed AND NOT EXISTS (SELECT 1 FROM acc_gl_journal_entry rev WHERE rev.reversal_id = gl.id) THEN gl.amount ELSE 0 END), 0)::numeric(19,6) AS credit_total,
    (COALESCE(SUM(CASE WHEN gl.type_enum = 2 AND NOT gl.reversed AND NOT EXISTS (SELECT 1 FROM acc_gl_journal_entry rev WHERE rev.reversal_id = gl.id) THEN gl.amount ELSE 0 END), 0)
     - COALESCE(SUM(CASE WHEN gl.type_enum = 1 AND NOT gl.reversed AND NOT EXISTS (SELECT 1 FROM acc_gl_journal_entry rev WHERE rev.reversal_id = gl.id) THEN gl.amount ELSE 0 END), 0))::numeric(19,6) AS balance,
    COUNT(*)::bigint AS entry_count,
    COUNT(*) FILTER (WHERE gl.reversed)::bigint AS reversed_entry_count,
    COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM acc_gl_journal_entry rev WHERE rev.reversal_id = gl.id))::bigint AS reversal_entry_count,
    COUNT(*) FILTER (WHERE gl.manual_entry)::bigint AS manual_entry_count
FROM acc_gl_journal_entry gl
JOIN acc_gl_account a ON a.id = gl.account_id
WHERE gl.office_id = ANY($1::bigint[])
  AND ($2::varchar IS NOT NULL OR $3::varchar IS NOT NULL)
  AND ($2::varchar IS NULL OR a.gl_code = $2::varchar)
  AND ($3::varchar IS NULL OR a.name = $3::varchar)
  AND gl.entry_date >= $4::date
  AND gl.entry_date <= $5::date
GROUP BY gl.office_id, gl.currency_code
ORDER BY gl.office_id, gl.currency_code
