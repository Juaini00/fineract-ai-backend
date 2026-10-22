-- params: {"from_date": "2026-04-01", "to_date": "2026-06-30", "limit": 200}
-- FDRD-2 / FDRD-6: fixed and recurring deposit terms and preclosure details for authorized offices.
-- Written independently of queries/fixed_recurring_deposit/fd_rd_term_preclosure.sql:
-- maturity_amount is a projection, on_account_closure_enum is a closure instruction.
WITH client_owner AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
)
SELECT dat.id AS term_id,
       sa.id AS savings_account_id,
       sa.client_id,
       co.office_id,
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
JOIN client_owner co ON co.client_id = sa.client_id
WHERE sa.deposit_type_enum IN (200, 300)
  AND (dat.maturity_date >= DATE '2026-04-01' AND dat.maturity_date <= DATE '2026-06-30')
ORDER BY co.office_id, sa.client_id, dat.maturity_date, dat.id
LIMIT 200
