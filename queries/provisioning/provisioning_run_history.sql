-- D07: recorded provisioning run history.
-- Org-wide reference query (grain: history_id).
-- Surfaces processing date (created_date), recording status (journal_entry_created), and creator/modifier IDs.
-- No simulation and no computation.
SELECT
    id AS history_id,
    created_date,
    createdby_id,
    journal_entry_created,
    lastmodifiedby_id,
    lastmodified_date
FROM m_provisioning_history
WHERE ($1::date IS NULL OR created_date >= $1::date)
  AND ($2::date IS NULL OR created_date <= $2::date)
ORDER BY created_date DESC, id DESC
LIMIT $3
