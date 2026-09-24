-- params: {"search": "Dao"}
-- Pencarian nasabah berdasarkan nama; display_name dipakai untuk ORDER BY
-- meski kolomnya sendiri ditahan (PII), sama seperti perilaku job. Ditulis
-- independen dari queries/client/name_lookup.sql: status_label diresolusi
-- lewat VALUES list yang ditulis dari
-- knowledge/schema/fineract/enums/client_status.yaml (kolom code:), bukan
-- CASE produksi; scope kantor lewat EXISTS, bukan JOIN ke m_office untuk
-- predikat (join tetap dipakai hanya untuk mengambil office_name).
WITH status_labels(status_enum, status_label) AS (
    VALUES (100, 'pending'), (300, 'active'), (600, 'closed')
)
SELECT
    cl.id AS client_id,
    o.name AS office_name,
    coalesce(sl.status_label, 'other') AS status_label
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
LEFT JOIN status_labels sl ON sl.status_enum = cl.status_enum
WHERE EXISTS (
    SELECT 1 FROM m_office ao WHERE ao.id = cl.office_id AND ao.id = ANY(:'office_ids'::bigint[])
  )
  AND cl.display_name ILIKE '%Dao%'
ORDER BY cl.display_name ASC, cl.id ASC
