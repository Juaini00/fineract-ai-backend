-- params: {}
-- Ringkasan hierarki kantor dihitung lewat CTE rekursif berbasis parent_id,
-- BUKAN dari string m_office.hierarchy (queries/organization/hierarchy_summary.sql
-- memakai hierarchy). Dua teknik berbeda, makna sama (I2: root berkedalaman 0).
WITH RECURSIVE depth_walk AS (
    SELECT o.id, 0::bigint AS depth
    FROM m_office o
    WHERE o.id = ANY(:'office_ids'::bigint[])
      AND o.parent_id IS NULL
    UNION ALL
    SELECT child.id, dw.depth + 1
    FROM m_office child
    JOIN depth_walk dw ON child.parent_id = dw.id
    WHERE child.id = ANY(:'office_ids'::bigint[])
)
SELECT
    (SELECT count(*) FROM m_office WHERE id = ANY(:'office_ids'::bigint[]))::bigint
        AS total_office_count,
    (SELECT count(*) FROM m_office WHERE id = ANY(:'office_ids'::bigint[]) AND parent_id IS NULL)::bigint
        AS root_office_count,
    (SELECT count(*)
       FROM m_office o
      WHERE o.id = ANY(:'office_ids'::bigint[])
        AND NOT EXISTS (
            SELECT 1 FROM m_office c
            WHERE c.parent_id = o.id AND c.id = ANY(:'office_ids'::bigint[])
        ))::bigint AS leaf_office_count,
    coalesce((SELECT max(depth) FROM depth_walk), 0)::bigint AS max_hierarchy_depth
