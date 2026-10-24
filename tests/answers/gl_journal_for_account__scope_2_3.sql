-- params: {"gl_code": null, "account_name": null, "from_date": null, "to_date": null, "limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-112 D12: general ledger journal debit/credit entries in offices 2 and 3.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(ARRAY[2,3]::bigint[])
),
account_ref AS (
    SELECT id AS account_id, gl_code, name AS account_name, classification_enum
    FROM acc_gl_account
)
SELECT
    gl.id AS journal_entry_id,
    gl.account_id,
    ar.gl_code,
    ar.account_name,
    ar.classification_enum::int AS classification_enum,
    gl.office_id,
    gl.currency_code,
    gl.transaction_id,
    gl.entry_date,
    gl.type_enum::int AS entry_type,
    gl.amount,
    gl.reversed,
    gl.reversal_id,
    EXISTS (SELECT 1 FROM acc_gl_journal_entry rev WHERE rev.reversal_id = gl.id) AS is_reversal_entry,
    gl.manual_entry,
    gl.loan_transaction_id,
    gl.savings_transaction_id,
    gl.client_transaction_id,
    gl.share_transaction_id
FROM acc_gl_journal_entry gl
JOIN scoped_offices so ON so.office_id = gl.office_id
JOIN account_ref ar ON ar.account_id = gl.account_id
ORDER BY gl.office_id, gl.entry_date DESC, gl.id DESC
LIMIT 200
