-- FDRD-1 / FDRD-4: fixed and recurring deposit accounts (grain: savings_account_id).
-- Filtered to deposit_type_enum IN (200, 300) where 200 = Fixed Deposit and 300 = Recurring Deposit
-- (live Fineract values resolved from m_savings_account and m_savings_product).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
-- Nominal interest rate and total interest posted derived are recorded facts;
-- no periodic schedule is imposed on FD.
SELECT
    sa.id AS savings_account_id,
    sa.client_id,
    c.office_id,
    sa.deposit_type_enum::bigint AS deposit_type_enum,
    sa.status_enum::bigint AS status_enum,
    sa.product_id,
    sa.currency_code,
    sa.nominal_annual_interest_rate,
    sa.total_interest_posted_derived,
    sa.submittedon_date,
    sa.activatedon_date,
    sa.closedon_date
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.deposit_type_enum IN (200, 300)
  AND (sa.submittedon_date >= $2::date AND sa.submittedon_date <= $3::date)
ORDER BY c.office_id, sa.client_id, sa.id
LIMIT $4
