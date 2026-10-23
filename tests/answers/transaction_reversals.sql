-- params: {"limit": 200}
-- FIN-130 LINK-7: transaction reversals and correction linkage in authorized offices.
-- Written independently of queries/linking_resources/transaction_reversals.sql:
-- per-domain UNION ALL across loan, savings, and general ledger transaction records.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
)
SELECT
    'loan'::text AS entity_type,
    lt.id AS transaction_id,
    lt.loan_id AS account_id,
    lt.office_id,
    lt.transaction_date,
    lt.amount,
    lt.is_reversed,
    false AS is_reversal,
    lt.reversed_on_date,
    NULL::bigint AS original_transaction_id,
    l.currency_code
FROM m_loan_transaction lt
JOIN m_loan l ON l.id = lt.loan_id
JOIN scoped_offices so ON so.office_id = lt.office_id

UNION ALL

SELECT
    'savings'::text AS entity_type,
    sat.id AS transaction_id,
    sat.savings_account_id AS account_id,
    sat.office_id,
    sat.transaction_date,
    sat.amount,
    sat.is_reversed,
    sat.is_reversal,
    NULL::date AS reversed_on_date,
    sat.original_transaction_id,
    sa.currency_code
FROM m_savings_account_transaction sat
JOIN m_savings_account sa ON sa.id = sat.savings_account_id
JOIN scoped_offices so ON so.office_id = sat.office_id

UNION ALL

SELECT
    'gl'::text AS entity_type,
    gl.id AS transaction_id,
    gl.account_id,
    gl.office_id,
    gl.entry_date AS transaction_date,
    gl.amount,
    gl.reversed AS is_reversed,
    (gl.reversal_id IS NOT NULL) AS is_reversal,
    NULL::date AS reversed_on_date,
    gl.reversal_id AS original_transaction_id,
    gl.currency_code
FROM acc_gl_journal_entry gl
JOIN scoped_offices so ON so.office_id = gl.office_id

ORDER BY office_id, transaction_date DESC, entity_type, transaction_id DESC
LIMIT 200
