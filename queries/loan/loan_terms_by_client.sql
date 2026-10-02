SELECT
    l.id AS loan_id,
    l.client_id,
    l.product_id,
    l.currency_code,
    l.principal_amount AS account_principal_amount,
    l.nominal_interest_rate_per_period AS account_interest_rate_per_period,
    l.interest_period_frequency_enum::bigint AS account_interest_period_frequency_enum,
    l.term_frequency::bigint AS account_term_frequency,
    l.term_period_frequency_enum::bigint AS account_term_period_frequency_enum,
    l.number_of_repayments::bigint AS account_number_of_repayments,
    p.principal_amount AS product_default_principal_amount,
    p.nominal_interest_rate_per_period AS product_default_interest_rate_per_period,
    p.number_of_repayments::bigint AS product_default_number_of_repayments
FROM m_loan l
JOIN m_product_loan p ON p.id = l.product_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
  AND l.client_id = $2::bigint
ORDER BY l.id
