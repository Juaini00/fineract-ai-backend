-- CLI-6: contact / address (selective) (grain: client_address_id).
-- Joins m_client_address -> m_client, m_address, and m_code_value for address type.
-- Office scope is the client's office (m_client.office_id = ANY($1::bigint[])).
-- Address detail fields carry pii sensitivity.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    ca.id AS client_address_id,
    ca.client_id,
    c.office_id,
    ca.address_id,
    ca.address_type_id::bigint AS address_type_id,
    cv.code_value AS address_type,
    ca.is_active,
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
FROM m_client_address ca
JOIN m_client c ON c.id = ca.client_id
JOIN m_address a ON a.id = ca.address_id
LEFT JOIN m_code_value cv ON cv.id = ca.address_type_id
WHERE c.office_id = ANY($1::bigint[])
ORDER BY c.office_id, ca.client_id, ca.id
LIMIT $2
