-- params: {"limit": 200}
-- NARROWED to offices 2 and 3: the job body carries office_ids [2,3].
-- FIN-128 CLI-5: all account types (loan, savings, share) owned by clients in offices 2 and 3.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
),
group_scope AS (
    SELECT id AS group_id, office_id
    FROM m_group
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
)
SELECT
    l.id AS account_id,
    'loan'::text AS account_type,
    l.client_id,
    COALESCE(cs.office_id, gs.office_id) AS office_id,
    l.product_id,
    l.loan_status_id::bigint AS status_enum,
    l.submittedon_date AS submitted_date,
    l.currency_code
FROM m_loan l
LEFT JOIN client_scope cs ON cs.client_id = l.client_id
LEFT JOIN group_scope gs ON gs.group_id = l.group_id
WHERE COALESCE(cs.office_id, gs.office_id) IS NOT NULL

UNION ALL

SELECT
    sa.id AS account_id,
    'savings'::text AS account_type,
    sa.client_id,
    cs.office_id,
    sa.product_id,
    sa.status_enum::bigint AS status_enum,
    sa.submittedon_date AS submitted_date,
    sa.currency_code
FROM m_savings_account sa
JOIN client_scope cs ON cs.client_id = sa.client_id

UNION ALL

SELECT
    sha.id AS account_id,
    'share'::text AS account_type,
    sha.client_id,
    cs.office_id,
    sha.product_id,
    sha.status_enum::bigint AS status_enum,
    sha.submitted_date,
    sha.currency_code
FROM m_share_account sha
JOIN client_scope cs ON cs.client_id = sha.client_id

ORDER BY office_id, client_id, account_type, account_id
LIMIT 200
