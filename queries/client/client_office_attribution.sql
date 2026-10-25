-- D02: client office attribution with explicit current-office labeling.
-- Verification confirmed no historical office assignment table exists.
-- All attribution is labeled current_office and office_as_of_position is NULL.
-- Authorization is strictly governed by the client's current office (c.office_id = ANY($1::bigint[])).
-- Client names, account numbers, and external IDs are never selected (pii safe).
SELECT
    c.id AS client_id,
    c.office_id AS office_id,
    'current_office'::text AS attribution_mode,
    NULL::bigint AS office_as_of_position
FROM m_client c
WHERE c.office_id = ANY($1::bigint[])
ORDER BY c.office_id, c.id
LIMIT $2
