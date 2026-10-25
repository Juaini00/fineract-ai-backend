-- params: {"from_date": ":today-12m", "to_date": ":today", "limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-116 D07: recorded provisioning entries in offices 2 and 3.
SELECT
    e.id AS entry_id,
    e.history_id,
    e.criteria_id,
    e.currency_code,
    e.office_id,
    e.product_id,
    e.category_id,
    e.overdue_in_days,
    e.reseve_amount AS reserve_amount,
    e.liability_account,
    e.expense_account
FROM m_loanproduct_provisioning_entry e
JOIN m_provisioning_history h ON h.id = e.history_id
JOIN m_provisioning_criteria c ON c.id = e.criteria_id
WHERE e.office_id = ANY(ARRAY[2,3]::bigint[])
  AND h.created_date >= (:'today'::date - interval '12 months')::date
  AND h.created_date <= :'today'::date
ORDER BY e.office_id, h.created_date DESC, e.id DESC
LIMIT 200
