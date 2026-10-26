-- params: {"limit": 200}
-- NARROWED to offices 2 and 3: the job body carries office_ids [2,3].
-- D02 / FIN-122: client current office attribution in offices 2 and 3.
SELECT
    c.id AS client_id,
    c.office_id AS office_id,
    'current_office'::text AS attribution_mode,
    NULL::bigint AS office_as_of_position
FROM m_client c
WHERE c.office_id = ANY(ARRAY[2,3]::bigint[])
ORDER BY c.office_id, c.id
LIMIT 200
