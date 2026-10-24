-- D12: general ledger journal debit/credit entries for one named GL account.
-- Surfaces reversal state (reversed flag and correlated reversal entry) and transaction links.
-- Office scope is the journal entry office bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    gl.id AS journal_entry_id,
    gl.account_id,
    a.gl_code,
    a.name AS account_name,
    a.classification_enum::int AS classification_enum,
    gl.office_id,
    gl.currency_code,
    gl.transaction_id,
    gl.entry_date,
    gl.type_enum::int AS entry_type,
    gl.amount,
    gl.reversed,
    gl.reversal_id,
    EXISTS (SELECT 1 FROM acc_gl_journal_entry o WHERE o.reversal_id = gl.id) AS is_reversal_entry,
    gl.manual_entry,
    gl.loan_transaction_id,
    gl.savings_transaction_id,
    gl.client_transaction_id,
    gl.share_transaction_id
FROM acc_gl_journal_entry gl
JOIN acc_gl_account a ON a.id = gl.account_id
WHERE gl.office_id = ANY($1::bigint[])
  AND ($2::varchar IS NULL OR a.gl_code = $2::varchar)
  AND ($3::varchar IS NULL OR a.name = $3::varchar)
  AND ($4::date IS NULL OR gl.entry_date >= $4::date)
  AND ($5::date IS NULL OR gl.entry_date <= $5::date)
ORDER BY gl.office_id, gl.entry_date DESC, gl.id DESC
LIMIT $6
