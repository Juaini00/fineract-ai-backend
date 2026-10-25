-- D05: failed standing instruction execution attempts within an explicit date window.
-- History execution attempts with failed status only (grain: history_id).
-- Error log surfaced verbatim as recorded, never interpreted or paraphrased.
-- Absence of a transfer is never proof of failure; failure claims trace strictly to history.status = 'failed'.
-- Office authorization path: via either linked account's office path (from_office_id or to_office_id).
-- Status 3 (DELETED) instructions are excluded.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    h.id AS history_id,
    h.standing_instruction_id,
    si.name AS name,
    h.status,
    to_char(h.execution_time, 'YYYY-MM-DD"T"HH24:MI:SS.US') AS execution_time,
    h.amount,
    h.error_log
FROM m_account_transfer_standing_instructions_history h
JOIN m_account_transfer_standing_instructions si ON si.id = h.standing_instruction_id
JOIN m_account_transfer_details atd ON atd.id = si.account_transfer_details_id
WHERE ((atd.from_office_id IS NOT NULL AND atd.from_office_id = ANY($1::bigint[]))
    OR (atd.to_office_id IS NOT NULL AND atd.to_office_id = ANY($1::bigint[])))
  AND si.status != 3
  AND h.status = 'failed'
  AND h.execution_time >= $2::date
  AND h.execution_time < $3::date + 1
ORDER BY h.execution_time DESC, h.id DESC
LIMIT $4
