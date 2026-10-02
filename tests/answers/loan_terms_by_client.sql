-- params: {"client_id": 65}
-- FIN-108 LOAN-3: account terms beside product's current default, client 65.
SELECT l.id AS loan_id, l.client_id, l.product_id, l.currency_code,
       l.principal_amount AS account_principal_amount,
       l.nominal_interest_rate_per_period AS account_interest_rate_per_period,
       l.interest_period_frequency_enum::bigint AS account_interest_period_frequency_enum,
       l.term_frequency::bigint AS account_term_frequency,
       l.term_period_frequency_enum::bigint AS account_term_period_frequency_enum,
       l.number_of_repayments::bigint AS account_number_of_repayments,
       (SELECT p.principal_amount FROM m_product_loan p WHERE p.id = l.product_id) AS product_default_principal_amount,
       (SELECT p.nominal_interest_rate_per_period FROM m_product_loan p WHERE p.id = l.product_id) AS product_default_interest_rate_per_period,
       (SELECT p.number_of_repayments::bigint FROM m_product_loan p WHERE p.id = l.product_id) AS product_default_number_of_repayments
FROM m_loan l
WHERE l.client_id = 65
  AND EXISTS (SELECT 1 FROM m_client c WHERE c.id = l.client_id AND c.office_id = ANY(:'office_ids'::bigint[]))
ORDER BY l.id
