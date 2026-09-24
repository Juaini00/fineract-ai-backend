-- params: {"limit": 500}
-- Pohon hierarki kantor, terurut depth-first per PATH (organization/
-- office_hierarchy_tree.yaml: "ordered by path") — bukan per level seperti
-- versi sebelumnya (ORDER BY depth ASC, office_id ASC, yang secara diam-diam
-- sama dengan urutan path HANYA karena setiap kantor hari ini anak langsung
-- Head Office; salah begitu ada cucu). Kunci urut dibangun lewat CTE
-- rekursif sebagai STRING ber-segmen lebar tetap (lpad 12 digit per id,
-- disambung '.'), bukan ARRAY[office_id] seperti
-- queries/organization/office_hierarchy_tree.sql — perbandingan string
-- leksikografis atas segmen berlebar tetap menghasilkan urutan depth-first
-- yang sama persis dengan perbandingan array elemen demi elemen, tanpa
-- bergantung pada tipe array Postgres.
-- Dibatasi row cap 500 (organization/office_hierarchy_tree.yaml
-- hard_cap=500, FIN-133; lihat
-- organization_office_hierarchy_tree__population.sql).
WITH RECURSIVE tree AS (
    SELECT o.id AS office_id, o.name AS office_name, o.parent_id, 1::bigint AS depth,
           lpad(o.id::text, 12, '0') AS sort_key
    FROM m_office o
    WHERE o.id = ANY(:'office_ids'::bigint[])
      AND o.parent_id IS NULL
    UNION ALL
    SELECT child.id, child.name, child.parent_id, tr.depth + 1,
           tr.sort_key || '.' || lpad(child.id::text, 12, '0')
    FROM m_office child
    JOIN tree tr ON child.parent_id = tr.office_id
    WHERE child.id = ANY(:'office_ids'::bigint[])
)
SELECT office_id, office_name, parent_id, depth
FROM tree
ORDER BY sort_key
LIMIT 500
