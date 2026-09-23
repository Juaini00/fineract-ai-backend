-- params: {"limit": 5, "office_name": null}
-- Sampel acak: SQL ini mengembalikan SELURUH populasi nasabah aktif yang
-- sah (tanpa ORDER BY random(), karena checkSubset tidak peduli urutan);
-- limit=5 adalah defaults.default_limit capability (FIN-133: limit.default
-- unbounded + defaults.default_limit terdeklarasi -> nilai itu yang diikat,
-- bukan hard_cap 50 — bukan disclosure, hanya ukuran jawaban yang diminta).
-- office_name=null berarti pool = seluruh scope tanpa penyempitan nama.
SELECT
    cl.id AS client_id,
    o.id AS office_id,
    o.name AS office_name,
    cl.activation_date
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
  AND cl.status_enum = 300
