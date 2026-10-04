-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-11: every recorded terms-change row (mapped to a
-- reschedule or not; mapping pre-aggregated + LEFT JOIN), scope
-- via a UNION of in-scope loan ids.
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    v.id AS term_variation_id,
    v.loan_id,
    mm.loan_reschedule_request_id,
    coalesce(mm.n, 0) AS reschedule_mapping_count,
    v.term_type,
    CASE WHEN v.term_type = 1 THEN 'emi_amount'
         WHEN v.term_type = 2 THEN 'interest_rate'
         WHEN v.term_type = 3 THEN 'principal_amount'
         WHEN v.term_type = 4 THEN 'due_date'
         WHEN v.term_type = 5 THEN 'insert_installment'
         WHEN v.term_type = 6 THEN 'delete_installment'
         WHEN v.term_type = 7 THEN 'grace_on_interest'
         WHEN v.term_type = 8 THEN 'grace_on_principal'
         WHEN v.term_type = 9 THEN 'extend_repayment_period'
         WHEN v.term_type = 10 THEN 'interest_rate_from_installment'
         WHEN v.term_type = 11 THEN 'interest_pause'
         ELSE 'other'
    END AS term_type_label,
    v.applicable_date,
    v.decimal_value,
    v.date_value,
    v.is_specific_to_installment,
    v.is_active
FROM m_loan_term_variations v
LEFT JOIN (
    SELECT loan_term_variations_id, min(loan_reschedule_request_id) AS loan_reschedule_request_id, count(*) AS n
    FROM m_loan_reschedule_request_term_variations_mapping
    GROUP BY loan_term_variations_id
) mm ON mm.loan_term_variations_id = v.id
JOIN scoped s ON s.id = v.loan_id
ORDER BY v.loan_id, v.applicable_date DESC, v.id DESC
LIMIT 100
