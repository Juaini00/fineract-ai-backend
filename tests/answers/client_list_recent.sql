-- params: {"limit": 50, "office_name": null}
-- Semua nasabah aktif dalam scope, terurut aktivasi terbaru dulu; limit=50
-- adalah defaults.default_limit capability (client/client_list_recent.yaml,
-- FIN-133: limit.default unbounded + defaults.default_limit terdeklarasi ->
-- nilai itu yang diikat, bukan hard_cap 200 — bukan disclosure). office_name
-- tidak terikat dari teks bebas sehingga tidak menyempitkan office manapun.
-- Ditulis independen dari queries/client/client_list_recent.sql: status
-- "active" diresolusi lewat VALUES list yang ditulis dari
-- knowledge/schema/fineract/enums/client_status.yaml (code: active), bukan
-- status_enum = 300 langsung di WHERE.
WITH status_labels(status_enum, status_code) AS (
    VALUES (100, 'pending'), (300, 'active'), (600, 'closed')
),
active_clients AS (
    SELECT cl.id, cl.office_id, cl.activation_date
    FROM m_client cl
    JOIN status_labels sl ON sl.status_enum = cl.status_enum
    WHERE sl.status_code = 'active'
      AND cl.activation_date IS NOT NULL
)
SELECT
    ac.id AS client_id,
    o.id AS office_id,
    o.name AS office_name,
    ac.activation_date
FROM active_clients ac
JOIN m_office o ON o.id = ac.office_id
WHERE ac.office_id = ANY(:'office_ids'::bigint[])
ORDER BY ac.activation_date DESC, ac.id DESC
LIMIT 50
