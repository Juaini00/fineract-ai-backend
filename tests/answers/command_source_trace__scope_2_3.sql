-- params: {"client_id": null, "loan_id": null, "savings_account_id": 37, "group_id": null, "resource_id": null, "entity_name": null, "action_name": null, "from_date": ":today-12m", "to_date": ":today", "limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): savings account 37 is in office 4, so returns 0 rows.
-- FIN-118 D09: action trace for savings account 37 in offices 2 and 3.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(ARRAY[2, 3]::bigint[])
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
WHERE cs.savings_account_id = 37
  AND cs.made_on_date_utc >= (:'today'::date - interval '12 months')::date
  AND cs.made_on_date_utc < (:'today'::date + 1)
ORDER BY cs.office_id, cs.made_on_date_utc DESC, cs.id DESC
LIMIT 200
