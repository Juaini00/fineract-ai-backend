-- D09: action trace for ONE NAMED resource.
-- Grain: [id].
-- Office scope: cs.office_id = ANY($1::bigint[]).
-- Named-subject guard: requires at least one of client_id, loan_id, savings_account_id, group_id, resource_id.
-- Returns 0 rows if no subject is provided.
-- Raw payload columns (command_as_json, result, client_ip, api_get_url, *_external_id) are never selected.
SELECT
    cs.id,
    cs.action_name,
    cs.entity_name,
    cs.office_id,
    cs.maker_id,
    mk.username AS maker_username,
    cs.made_on_date_utc,
    cs.checker_id,
    ck.username AS checker_username,
    cs.checked_on_date_utc,
    cs.status,
    cs.result_status_code,
    cs.client_id,
    cs.loan_id,
    cs.savings_account_id,
    cs.group_id,
    cs.product_id,
    cs.resource_id,
    cs.subresource_id,
    cs.transaction_id
FROM m_portfolio_command_source cs
JOIN m_appuser mk ON mk.id = cs.maker_id
LEFT JOIN m_appuser ck ON ck.id = cs.checker_id
WHERE cs.office_id = ANY($1::bigint[])
  AND (
    $2::bigint IS NOT NULL
    OR $3::bigint IS NOT NULL
    OR $4::bigint IS NOT NULL
    OR $5::bigint IS NOT NULL
    OR $6::bigint IS NOT NULL
  )
  AND ($2::bigint IS NULL OR cs.client_id = $2::bigint)
  AND ($3::bigint IS NULL OR cs.loan_id = $3::bigint)
  AND ($4::bigint IS NULL OR cs.savings_account_id = $4::bigint)
  AND ($5::bigint IS NULL OR cs.group_id = $5::bigint)
  AND ($6::bigint IS NULL OR cs.resource_id = $6::bigint)
  AND ($7::varchar IS NULL OR cs.entity_name = $7::varchar)
  AND ($8::varchar IS NULL OR cs.action_name = $8::varchar)
  AND ($9::date IS NULL OR cs.made_on_date_utc >= $9::date)
  AND ($10::date IS NULL OR cs.made_on_date_utc < $10::date + 1)
ORDER BY cs.office_id, cs.made_on_date_utc DESC, cs.id DESC
LIMIT $11
