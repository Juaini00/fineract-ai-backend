-- params: {"from_date": "2025-11-01", "to_date": "2026-04-30", "limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-111 SHARE-1..3, 7: share accounts in offices 2 and 3.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
),
product_ref AS (
    SELECT id AS product_id, name AS product_name, currency_code AS product_currency_code
    FROM m_share_product
)
SELECT
    sa.id AS account_id,
    sa.client_id,
    cs.office_id,
    sa.product_id,
    pr.product_name,
    pr.product_currency_code,
    sa.status_enum::bigint AS status_enum,
    sa.submitted_date,
    sa.approved_date,
    sa.rejected_date,
    sa.activated_date,
    sa.closed_date,
    sa.currency_code,
    sa.currency_digits::integer AS currency_digits,
    sa.currency_multiplesof::integer AS currency_multiplesof,
    sa.total_approved_shares,
    sa.total_pending_shares,
    sa.minimum_active_period_frequency,
    sa.minimum_active_period_frequency_enum::bigint AS minimum_active_period_frequency_enum,
    sa.lockin_period_frequency,
    sa.lockin_period_frequency_enum::bigint AS lockin_period_frequency_enum,
    sa.allow_dividends_inactive_clients,
    sa.savings_account_id
FROM m_share_account sa
JOIN client_scope cs ON cs.client_id = sa.client_id
JOIN product_ref pr ON pr.product_id = sa.product_id
WHERE sa.submitted_date >= DATE '2025-11-01' AND sa.submitted_date <= DATE '2026-04-30'
ORDER BY cs.office_id, sa.client_id, sa.id
LIMIT 200
