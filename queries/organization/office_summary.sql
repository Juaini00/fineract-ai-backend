-- Jumlah staf aktif diambil lewat subquery berkorelasi, BUKAN LEFT JOIN m_staff.
-- LEFT JOIN menggandakan baris kantor sebanyak jumlah stafnya sehingga
-- COUNT(o.id) menghitung pasangan (kantor, staf), bukan kantor.
-- Diverifikasi 2026-09-15: bentuk lama mengembalikan office_count = 27 untuk 8 kantor.
SELECT
    COUNT(o.id)::bigint AS office_count,
    COUNT(o.id) FILTER (WHERE o.parent_id IS NULL)::bigint AS root_office_count,
    MIN(o.opening_date) AS oldest_opening_date,
    COALESCE(SUM((
        SELECT COUNT(*)
        FROM m_staff s
        WHERE s.office_id = o.id
          AND s.is_active = true
    )), 0)::bigint AS active_staff_count
FROM m_office o
WHERE o.id = ANY($1::bigint[])
;
