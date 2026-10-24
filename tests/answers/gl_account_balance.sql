-- params: {"gl_code": "20101", "account_name": null, "from_date": ":today-12m", "to_date": ":today"}
-- FIN-112 D12: general ledger account balance and turnover in authorized offices.
-- Written independently of queries/accounting/gl_account_balance.sql:
-- office scope resolved via CTE join, reversal flags checked via subqueries.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
)
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
JOIN scoped_offices so ON so.office_id = gl.office_id
JOIN acc_gl_account a ON a.id = gl.account_id
WHERE a.gl_code = '20101'
  AND gl.entry_date >= (:'today'::date - interval '12 months')::date
  AND gl.entry_date <= :'today'::date
GROUP BY gl.office_id, gl.currency_code
ORDER BY gl.office_id, gl.currency_code
