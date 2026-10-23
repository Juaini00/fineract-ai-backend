-- params: {"limit": 200}
-- FIN-128 CLI-6: client addresses in authorized offices.
-- Written independently of queries/client/client_address.sql:
-- client scope resolved via client_scope CTE, address details through address_ref CTE.
WITH client_scope AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
),
address_ref AS (
    SELECT
        a.id,
        a.street,
        a.address_line_1,
        a.address_line_2,
        a.address_line_3,
        a.town_village,
        a.city,
        a.county_district,
        a.state_province_id::bigint AS state_province_id,
        a.country_id::bigint AS country_id,
        a.postal_code,
        a.latitude,
        a.longitude
    FROM m_address a
),
code_ref AS (
    SELECT id, code_value
    FROM m_code_value
)
SELECT
    ca.id AS client_address_id,
    ca.client_id,
    cs.office_id,
    ca.address_id,
    ca.address_type_id::bigint AS address_type_id,
    cr.code_value AS address_type,
    ca.is_active,
    ar.street,
    ar.address_line_1,
    ar.address_line_2,
    ar.address_line_3,
    ar.town_village,
    ar.city,
    ar.county_district,
    ar.state_province_id,
    ar.country_id,
    ar.postal_code,
    ar.latitude,
    ar.longitude
FROM m_client_address ca
JOIN client_scope cs ON cs.client_id = ca.client_id
JOIN address_ref ar ON ar.id = ca.address_id
LEFT JOIN code_ref cr ON cr.id = ca.address_type_id
ORDER BY cs.office_id, ca.client_id, ca.id
LIMIT 200
