-- CLI-4: client group and center membership (grain: client_id, group_id).
-- Joins m_group_client -> m_group -> m_client.
-- Authorization is governed by the client's office (m_client.office_id = ANY($1::bigint[])).
-- Group office is surfaced as evidence only.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    gc.client_id,
    c.office_id AS client_office_id,
    gc.group_id,
    g.display_name AS group_name,
    g.level_id::bigint AS level_id,
    g.parent_id,
    g.office_id AS group_office_id
FROM m_group_client gc
JOIN m_group g ON g.id = gc.group_id
JOIN m_client c ON c.id = gc.client_id
WHERE c.office_id = ANY($1::bigint[])
ORDER BY c.office_id, gc.client_id, gc.group_id
LIMIT $2
