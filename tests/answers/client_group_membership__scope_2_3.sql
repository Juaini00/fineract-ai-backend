-- params: {"limit": 200}
-- NARROWED to offices 2 and 3: the job body carries office_ids [2,3].
-- FIN-128 CLI-4: client group membership in offices 2 and 3.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
),
group_ref AS (
    SELECT id AS group_id, display_name AS group_name, level_id, parent_id, office_id AS group_office_id
    FROM m_group
)
SELECT
    gc.client_id,
    cs.office_id AS client_office_id,
    gc.group_id,
    gr.group_name,
    gr.level_id::bigint AS level_id,
    gr.parent_id,
    gr.group_office_id
FROM m_group_client gc
JOIN client_scope cs ON cs.client_id = gc.client_id
JOIN group_ref gr ON gr.group_id = gc.group_id
ORDER BY cs.office_id, gc.client_id, gc.group_id
LIMIT 200
