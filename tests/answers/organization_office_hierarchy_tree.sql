-- params: {"limit": null}
-- Pohon hierarki kantor, akar dulu lalu turun per level; kedalaman akar = 1
-- (padanan queries/organization/office_hierarchy_tree.sql yang memulai
-- depth dari 1), ditulis lewat CTE rekursif terpisah tanpa array path.
WITH RECURSIVE tree AS (
    SELECT o.id AS office_id, o.name AS office_name, o.parent_id, 1::bigint AS depth
    FROM m_office o
    WHERE o.id = ANY(:'office_ids'::bigint[])
      AND o.parent_id IS NULL
    UNION ALL
    SELECT child.id, child.name, child.parent_id, tr.depth + 1
    FROM m_office child
    JOIN tree tr ON child.parent_id = tr.office_id
    WHERE child.id = ANY(:'office_ids'::bigint[])
)
SELECT office_id, office_name, parent_id, depth
FROM tree
ORDER BY depth ASC, office_id ASC
