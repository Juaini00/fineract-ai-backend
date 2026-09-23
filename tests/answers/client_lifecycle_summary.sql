-- params: {}
-- Distribusi status siklus hidup nasabah dalam scope, satu baris ringkasan.
SELECT
    count(*) AS client_count,
    count(*) FILTER (WHERE cl.status_enum = 300) AS active_client_count,
    count(*) FILTER (WHERE cl.status_enum = 100) AS pending_client_count,
    count(*) FILTER (WHERE cl.status_enum = 600) AS closed_client_count
FROM m_client cl
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
