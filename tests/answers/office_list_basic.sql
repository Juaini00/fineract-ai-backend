-- params: {"limit": null}
-- Daftar kantor pada cakupan otorisasi, terlama dibuka dulu.
SELECT
    o.id AS office_id,
    o.name AS office_name,
    o.parent_id,
    o.opening_date
FROM m_office o
WHERE o.id = ANY(:'office_ids'::bigint[])
ORDER BY o.opening_date ASC NULLS LAST, o.id ASC
