-- D07: recorded provisioning entries per office, product, and overdue bucket.
-- Grain: entry_id.
-- Office scope: e.office_id = ANY($1::bigint[]).
-- The misspelled column reseve_amount is aliased to reserve_amount.
-- No loan-level contributor synthesis and no simulation.
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
WHERE e.office_id = ANY($1::bigint[])
  AND ($2::date IS NULL OR h.created_date >= $2::date)
  AND ($3::date IS NULL OR h.created_date <= $3::date)
ORDER BY e.office_id, h.created_date DESC, e.id DESC
LIMIT $4
