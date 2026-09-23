-- params: {}
-- Jumlah rekening tabungan per nasabah (seluruh status), satu baris per
-- klien; hanya office_ids yang terikat capability ini.
SELECT
    cl.id AS client_id,
    count(sav.id) AS savings_account_count
FROM m_client cl
LEFT JOIN m_savings_account sav ON sav.client_id = cl.id
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
GROUP BY cl.id
