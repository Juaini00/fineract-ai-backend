-- SAV-9: savings accounts with withheld tax (grain: account_id).
-- Distinguishes tax config (PROD-10) from tax actually withheld on the account (D04 separation pattern).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- On this deployment, withhold_tax is false for all live accounts (genuine Empty outcome).
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sa.id AS account_id,
    sa.client_id,
    c.office_id,
    sa.product_id,
    sa.withhold_tax,
    sa.tax_group_id,
    tg.name AS tax_group_name,
    sa.total_withhold_tax_derived,
    sa.currency_code
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
LEFT JOIN m_tax_group tg ON tg.id = sa.tax_group_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.withhold_tax = true
ORDER BY c.office_id, sa.client_id, sa.id
LIMIT $2
