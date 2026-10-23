-- params: {"from_date": "2025-11-01", "to_date": "2026-04-30", "limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-127 SAV-6: savings interest positions in offices 2 and 3.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
),
product_ref AS (
    SELECT id AS product_id, name AS product_name
    FROM m_savings_product
)
SELECT
    sa.id AS account_id,
    sa.client_id,
    cs.office_id,
    sa.product_id,
    pr.product_name,
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
JOIN client_scope cs ON cs.client_id = sa.client_id
JOIN product_ref pr ON pr.product_id = sa.product_id
WHERE sa.submittedon_date >= DATE '2025-11-01' AND sa.submittedon_date <= DATE '2026-04-30'
ORDER BY cs.office_id, sa.client_id, sa.id
LIMIT 200
