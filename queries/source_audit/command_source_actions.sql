-- D09: recorded action ledger within authorized office scope.
-- Grain: [id].
-- Office scope: cs.office_id = ANY($1::bigint[]).
-- Actor attribution: maker_id joined to m_appuser (never officer assignment).
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
  AND ($2::varchar IS NULL OR cs.entity_name = $2::varchar)
  AND ($3::varchar IS NULL OR cs.action_name = $3::varchar)
  AND ($4::smallint IS NULL OR cs.status = $4::smallint)
  AND ($5::date IS NULL OR cs.made_on_date_utc >= $5::date)
  AND ($6::date IS NULL OR cs.made_on_date_utc < $6::date + 1)
ORDER BY cs.office_id, cs.made_on_date_utc DESC, cs.id DESC
LIMIT $7
