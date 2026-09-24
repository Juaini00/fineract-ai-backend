-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk organization_office_hierarchy_tree,
-- dipakai lib/answers.js::expectRowCap (FIN-133). CTE rekursif identik
-- dengan oracle utama MINUS LIMIT (sort_key tidak diperlukan untuk count).
WITH RECURSIVE tree AS (
    SELECT o.id AS office_id
    FROM m_office o
    WHERE o.id = ANY(:'office_ids'::bigint[])
      AND o.parent_id IS NULL
    UNION ALL
    SELECT child.id
    FROM m_office child
    JOIN tree tr ON child.parent_id = tr.office_id
    WHERE child.id = ANY(:'office_ids'::bigint[])
)
SELECT count(*) AS n FROM tree
