-- params: {"limit": 10000, "office_name": null}
-- Distribusi status siklus hidup nasabah per office, dibatasi row cap 10000
-- (client/summary_by_office.yaml hard_cap=10000, FIN-133; lihat
-- client_summary_by_office__population.sql), LATERAL per office alih-alih
-- JOIN+GROUP BY datar seperti queries/client/summary_by_office.sql; office
-- tanpa nasabah tersingkir secara natural sama seperti INNER JOIN asli.
SELECT
    o.id AS office_id,
    o.name AS office_name,
    stat.active_count,
    stat.pending_count,
    stat.closed_count,
    stat.total_count
FROM m_office o
JOIN LATERAL (
    SELECT
        count(*) FILTER (WHERE cl.status_enum = 300) AS active_count,
        count(*) FILTER (WHERE cl.status_enum = 100) AS pending_count,
        count(*) FILTER (WHERE cl.status_enum = 600) AS closed_count,
        count(*) AS total_count
    FROM m_client cl WHERE cl.office_id = o.id
) stat ON true
WHERE o.id = ANY(:'office_ids'::bigint[])
  AND stat.total_count > 0
ORDER BY stat.total_count DESC, o.id ASC
LIMIT 10000
