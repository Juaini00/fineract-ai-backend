-- D05: standing instruction recurrence schedule as recorded.
-- No next-run computation and no date arithmetic; surfaces recorded configuration verbatim.
-- Office authorization path: via either linked account's office path (from_office_id or to_office_id).
-- Status 3 (DELETED) rows are strictly excluded.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    si.id AS standing_instruction_id,
    si.name,
    atd.from_office_id,
    atd.to_office_id,
    si.recurrence_type::smallint AS recurrence_type,
    si.recurrence_frequency::smallint AS recurrence_frequency,
    si.recurrence_interval::smallint AS recurrence_interval,
    si.recurrence_on_day::smallint AS recurrence_on_day,
    si.recurrence_on_month::smallint AS recurrence_on_month,
    si.value_date_offset_days::integer AS value_date_offset_days,
    si.valid_from,
    si.valid_till,
    si.last_run_date
FROM m_account_transfer_standing_instructions si
JOIN m_account_transfer_details atd ON atd.id = si.account_transfer_details_id
WHERE ((atd.from_office_id IS NOT NULL AND atd.from_office_id = ANY($1::bigint[]))
    OR (atd.to_office_id IS NOT NULL AND atd.to_office_id = ANY($1::bigint[])))
  AND si.status != 3
ORDER BY atd.from_office_id, si.id
LIMIT $2
