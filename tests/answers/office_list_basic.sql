-- params: {"limit": 50}
-- Daftar kantor pada cakupan otorisasi, terlama dibuka dulu; limit=50 adalah
-- defaults.default_limit capability (organization/office_list_basic.yaml,
-- FIN-133: limit.default unbounded + defaults.default_limit terdeklarasi ->
-- nilai itu yang diikat, bukan hard_cap 200 — bukan disclosure, hanya
-- ukuran jawaban yang diminta). NULLS LAST ditulis lewat COALESCE ke
-- tanggal jauh di masa depan, bukan klausa `ORDER BY ... NULLS LAST`
-- langsung seperti queries/organization/office_list_basic.sql.
SELECT
    o.id AS office_id,
    o.name AS office_name,
    o.parent_id,
    o.opening_date
FROM m_office o
WHERE o.id = ANY(:'office_ids'::bigint[])
ORDER BY coalesce(o.opening_date, 'infinity'::date) ASC, o.id ASC
LIMIT 50
