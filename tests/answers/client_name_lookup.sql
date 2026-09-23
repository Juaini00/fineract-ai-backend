-- params: {"search": "Dao"}
-- Pencarian nasabah berdasarkan nama; display_name dipakai untuk ORDER BY
-- meski kolomnya sendiri ditahan (PII), sama seperti perilaku job.
SELECT
    cl.id AS client_id,
    o.name AS office_name,
    CASE cl.status_enum
        WHEN 100 THEN 'pending'
        WHEN 300 THEN 'active'
        WHEN 600 THEN 'closed'
        ELSE 'other'
    END AS status_label
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
  AND cl.display_name ILIKE '%Dao%'
ORDER BY cl.display_name ASC, cl.id ASC
