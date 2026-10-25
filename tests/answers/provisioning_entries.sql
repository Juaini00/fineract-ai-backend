-- params: {"from_date": ":today-12m", "to_date": ":today", "limit": 200}
-- FIN-116 D07: recorded provisioning entries in authorized offices.
-- Written independently of queries/provisioning/provisioning_entries.sql:
-- office resolved through scoped CTE, reseve_amount aliased to reserve_amount.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
)
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
JOIN scoped_offices so ON so.office_id = e.office_id
JOIN m_provisioning_history h ON h.id = e.history_id
JOIN m_provisioning_criteria c ON c.id = e.criteria_id
WHERE h.created_date >= (:'today'::date - interval '12 months')::date
  AND h.created_date <= :'today'::date
ORDER BY e.office_id, h.created_date DESC, e.id DESC
LIMIT 200
