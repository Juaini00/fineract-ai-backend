-- CLI-5: all account types (loan, savings, share) owned by clients (grain: account_type, account_id).
-- Per-client UNION ALL across m_loan, m_savings_account, m_share_account.
-- Account type surfaced as declared literal ('loan', 'savings', 'share').
-- Raw status enum preserved (loan loan_status_id, savings status_enum, share status_enum).
-- Submitted/opened date and currency code preserved per account type without mixing currencies.
-- No default-active filter (§1 rule 34).
-- Authorization: loan via client office (with group office fallback), savings/share via client office.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    l.id AS account_id,
    'loan'::text AS account_type,
    l.client_id,
    COALESCE(c.office_id, g.office_id) AS office_id,
    l.product_id,
    l.loan_status_id::bigint AS status_enum,
    l.submittedon_date AS submitted_date,
    l.currency_code
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])

UNION ALL

SELECT
    sa.id AS account_id,
    'savings'::text AS account_type,
    sa.client_id,
    c.office_id,
    sa.product_id,
    sa.status_enum::bigint AS status_enum,
    sa.submittedon_date AS submitted_date,
    sa.currency_code
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])

UNION ALL

SELECT
    sha.id AS account_id,
    'share'::text AS account_type,
    sha.client_id,
    c.office_id,
    sha.product_id,
    sha.status_enum::bigint AS status_enum,
    sha.submitted_date,
    sha.currency_code
FROM m_share_account sha
JOIN m_client c ON c.id = sha.client_id
WHERE c.office_id = ANY($1::bigint[])

ORDER BY office_id, client_id, account_type, account_id
LIMIT $2
