-- SAV-6: savings interest recorded versus posted position (grain: account_id).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- total_interest_earned_derived and total_interest_posted_derived are kept distinct,
-- proving earned-to-date is not the same as posted interest.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sa.id AS account_id,
    sa.client_id,
    c.office_id,
    sa.product_id,
    sp.name AS product_name,
    sa.currency_code,
    sa.nominal_annual_interest_rate,
    sa.interest_compounding_period_enum::bigint AS interest_compounding_period_enum,
    sa.interest_posting_period_enum::bigint AS interest_posting_period_enum,
    sa.interest_calculation_type_enum::bigint AS interest_calculation_type_enum,
    sa.interest_calculation_days_in_year_type_enum::bigint AS interest_calculation_days_in_year_type_enum,
    sa.total_interest_earned_derived,
    sa.total_interest_posted_derived,
    sa.interest_posted_till_date,
    sa.last_interest_calculation_date,
    sa.submittedon_date
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.submittedon_date >= $2::date AND sa.submittedon_date <= $3::date
ORDER BY c.office_id, sa.client_id, sa.id
LIMIT $4
