-- params: {"limit": 200}
-- FIN-128 CLI-4: client group membership in authorized offices.
-- Written independently of queries/client/client_group_membership.sql:
-- client scope resolved via client_scope CTE, group details through group_ref CTE.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
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
