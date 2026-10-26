-- params: {"limit": 200}
-- D02 / FIN-122: client current office attribution in authorized offices.
-- Written independently of queries/client/client_office_attribution.sql.
SELECT
    c.id AS client_id,
    c.office_id AS office_id,
    'current_office'::text AS attribution_mode,
    NULL::bigint AS office_as_of_position
FROM m_client c
WHERE c.office_id = ANY(:'office_ids'::bigint[])
ORDER BY c.office_id, c.id
LIMIT 200
