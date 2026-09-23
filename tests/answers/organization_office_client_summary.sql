-- params: {"limit": null, "office_name": null}
-- Jumlah nasabah per siklus hidup per kantor, termasuk kantor tanpa nasabah.
-- office_name dan limit tidak diisi (default) sehingga tanpa filter nama dan
-- tanpa batas baris.
SELECT
    o.id AS office_id,
    o.name AS office_name,
    count(c.id) FILTER (WHERE c.status_enum = 300)::bigint AS active_clients,
    count(c.id) FILTER (WHERE c.status_enum = 100)::bigint AS pending_clients,
    count(c.id) FILTER (WHERE c.status_enum = 600)::bigint AS closed_clients,
    count(c.id)::bigint AS total_clients
FROM m_office o
LEFT JOIN m_client c ON c.office_id = o.id
WHERE o.id = ANY(:'office_ids'::bigint[])
GROUP BY o.id, o.name
ORDER BY total_clients DESC, o.id ASC
