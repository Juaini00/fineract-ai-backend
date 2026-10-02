-- params: {"client_id": 65}
-- FIN-108 LOAN-1/LOAN-2: every loan of client 65 (all statuses), written
-- independently: scope checked with EXISTS over client or group office,
-- status label from Fineract's own r_enum_value instead of a CASE.
SELECT l.id AS loan_id, l.client_id, l.group_id, l.product_id, l.loan_officer_id, l.fund_id,
       l.currency_code, l.loan_status_id::bigint AS loan_status_id,
       CASE e.enum_value
           WHEN 'Submitted and awaiting approval' THEN 'submitted_and_pending_approval'
           ELSE lower(replace(replace(e.enum_value, ' ', '_'), '-', '_'))
       END AS loan_status,
       l.submittedon_date AS submitted_on_date, l.approvedon_date AS approved_on_date,
       l.disbursedon_date AS disbursed_on_date, l.closedon_date AS closed_on_date,
       l.rejectedon_date AS rejected_on_date, l.writtenoffon_date AS written_off_on_date
FROM m_loan l
LEFT JOIN r_enum_value e ON e.enum_name = 'loan_status_id' AND e.enum_id = l.loan_status_id
WHERE l.client_id = 65
  AND (EXISTS (SELECT 1 FROM m_client c WHERE c.id = l.client_id AND c.office_id = ANY(:'office_ids'::bigint[]))
       OR EXISTS (SELECT 1 FROM m_group g WHERE g.id = l.group_id AND l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])))
ORDER BY l.id
