-- params: {"entity_name": null, "action_name": null, "status": null, "from_date": ":today-12m", "to_date": ":today", "limit": 200}
-- FIN-118 D09: recorded action ledger within authorized office scope.
-- Written independently of queries/source_audit/command_source_actions.sql:
-- office resolved through scoped CTE, appuser resolved through user CTE.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
),
app_users AS (
    SELECT id AS user_id, username
    FROM m_appuser
)
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
JOIN scoped_offices so ON so.office_id = cs.office_id
JOIN app_users mk ON mk.user_id = cs.maker_id
LEFT JOIN app_users ck ON ck.user_id = cs.checker_id
WHERE cs.made_on_date_utc >= (:'today'::date - interval '12 months')::date
  AND cs.made_on_date_utc < (:'today'::date + 1)
ORDER BY cs.office_id, cs.made_on_date_utc DESC, cs.id DESC
LIMIT 200
