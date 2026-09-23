-- params: {"limit": null, "office_name": null}
-- Semua nasabah aktif dalam scope, terurut aktivasi terbaru dulu; office_name
-- tidak terikat dari teks bebas sehingga tidak menyempitkan office manapun.
SELECT
    cl.id AS client_id,
    o.id AS office_id,
    o.name AS office_name,
    cl.activation_date
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
  AND cl.status_enum = 300
  AND cl.activation_date IS NOT NULL
ORDER BY cl.activation_date DESC, cl.id DESC
