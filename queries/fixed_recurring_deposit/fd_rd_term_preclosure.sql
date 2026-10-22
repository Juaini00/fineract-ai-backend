-- FDRD-2 / FDRD-6: fixed and recurring deposit terms and preclosure details (grain: term_id).
-- One row per m_deposit_account_term_and_preclosure row joined to m_savings_account and m_client.
-- maturity_amount is a PROJECTION, never payout evidence; maturity_date is a projected date.
-- on_account_closure_enum is a stored instruction (100=withdraw, 200=transfer to savings,
-- 300=reinvest principal+interest, 400=reinvest principal only), NOT proof of payout or transfer.
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- Account numbers and external IDs are never selected.
SELECT
    dat.id AS term_id,
    sa.id AS savings_account_id,
    sa.client_id,
    c.office_id,
    sa.deposit_type_enum::bigint AS deposit_type_enum,
    sa.currency_code,
    dat.deposit_amount,
    dat.maturity_amount,
    dat.maturity_date,
    dat.min_deposit_term,
    dat.min_deposit_term_type_enum::bigint AS min_deposit_term_type_enum,
    dat.deposit_period,
    dat.deposit_period_frequency_enum::bigint AS deposit_period_frequency_enum,
    dat.pre_closure_penal_applicable,
    dat.pre_closure_penal_interest,
    dat.pre_closure_penal_interest_on_enum::bigint AS pre_closure_penal_interest_on_enum,
    dat.on_account_closure_enum::bigint AS on_account_closure_enum
FROM m_deposit_account_term_and_preclosure dat
JOIN m_savings_account sa ON sa.id = dat.savings_account_id
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.deposit_type_enum IN (200, 300)
  AND (dat.maturity_date >= $2::date AND dat.maturity_date <= $3::date)
ORDER BY c.office_id, sa.client_id, dat.maturity_date, dat.id
LIMIT $4
