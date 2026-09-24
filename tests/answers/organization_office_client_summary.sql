-- params: {"limit": 200, "office_name": null}
-- Jumlah nasabah per siklus hidup (client_status enum: 100=pending,
-- 300=active, 600=closed — knowledge/schema/fineract/enums/client_status.yaml)
-- per kantor, termasuk kantor tanpa nasabah, dibatasi row cap 200
-- (organization/office_client_summary.yaml hard_cap=200, FIN-133).
-- office_name tidak diisi (default) sehingga tanpa filter nama. Ditulis
-- independen dari queries/organization/office_client_summary.sql: empat
-- correlated subquery count() terpisah, bukan satu LEFT JOIN + count(...)
-- FILTER (WHERE ...). Lihat organization_office_client_summary__population.sql
-- untuk ukuran populasi penuh yang dipakai lib/answers.js::expectRowCap.
SELECT
    o.id AS office_id,
    o.name AS office_name,
    (SELECT count(*) FROM m_client c WHERE c.office_id = o.id AND c.status_enum = 300)::bigint AS active_clients,
    (SELECT count(*) FROM m_client c WHERE c.office_id = o.id AND c.status_enum = 100)::bigint AS pending_clients,
    (SELECT count(*) FROM m_client c WHERE c.office_id = o.id AND c.status_enum = 600)::bigint AS closed_clients,
    (SELECT count(*) FROM m_client c WHERE c.office_id = o.id)::bigint AS total_clients
FROM m_office o
WHERE o.id = ANY(:'office_ids'::bigint[])
ORDER BY (SELECT count(*) FROM m_client c WHERE c.office_id = o.id) DESC, o.id ASC
LIMIT 200
