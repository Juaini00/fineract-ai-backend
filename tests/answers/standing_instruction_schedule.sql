-- params: {"limit": 200}
-- FIN-114 D05: standing instruction recurrence schedule in authorized offices.
-- Written independently of queries/standing_instructions/standing_instruction_schedule.sql:
-- resolving office through CTE join, verbatim schedule configuration.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
)
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
WHERE (EXISTS (SELECT 1 FROM scoped_offices so WHERE so.office_id = atd.from_office_id)
    OR EXISTS (SELECT 1 FROM scoped_offices so WHERE so.office_id = atd.to_office_id))
  AND si.status != 3
ORDER BY atd.from_office_id, si.id
LIMIT 200
