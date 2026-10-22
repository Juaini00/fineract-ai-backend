-- SHARE-1 / SHARE-2 / SHARE-3 / SHARE-7: share account identity, effective terms,
-- ownership by status, and settlement linkage (grain: account_id).
-- Product reference joined to m_share_product (covering PROD-3) for name and currency reference only.
-- Status enum returned as raw bigint; lifecycle dates surfaced as recorded.
-- Ownership measure is total_approved_shares; total_pending_shares is reported separately and never counted as owned.
-- savings_account_id is surfaced as linkage only and never proves settlement of any purchase or redemption.
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sa.id AS account_id,
    sa.client_id,
    c.office_id,
    sa.product_id,
    sp.name AS product_name,
    sp.currency_code AS product_currency_code,
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
JOIN m_client c ON c.id = sa.client_id
JOIN m_share_product sp ON sp.id = sa.product_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.submitted_date >= $2::date AND sa.submitted_date <= $3::date
ORDER BY c.office_id, sa.client_id, sa.id
LIMIT $4
