-- Recorded terms-change rows (dataset-inventory.md LOAN-11): one row per
-- m_loan_term_variations entry, whether or not a reschedule request applied it
-- (Apache Fineract also writes variations from LoanScheduleAssembler at loan
-- application, LoanWritePlatformServiceJpaRepositoryImpl and interest pauses,
-- none of which create a reschedule mapping), never
-- inferred from schedule or balance deltas. Distinct grain from the reschedule
-- request itself (see loan.loan_reschedule). loan_reschedule_request_id is the
-- mapped request (NULL = not from a reschedule); it is read through a scalar
-- subquery and reschedule_mapping_count reports how many mappings exist, so a
-- duplicated mapping row can never fan the variation row out or be hidden.
-- term_type per Apache Fineract LoanTermVariationType (1..11, labels below).
-- is_active marks whether this variation is still the one in effect (a
-- later reschedule can supersede an earlier one). Office-scoped via the
-- owning loan's client-or-group office.
SELECT
    v.id AS term_variation_id,
    v.loan_id,
    (SELECT MIN(m.loan_reschedule_request_id)
       FROM m_loan_reschedule_request_term_variations_mapping m
      WHERE m.loan_term_variations_id = v.id) AS loan_reschedule_request_id,
    (SELECT COUNT(*)
       FROM m_loan_reschedule_request_term_variations_mapping m
      WHERE m.loan_term_variations_id = v.id) AS reschedule_mapping_count,
    v.term_type,
    CASE v.term_type
        WHEN 1 THEN 'emi_amount'
        WHEN 2 THEN 'interest_rate'
        WHEN 3 THEN 'principal_amount'
        WHEN 4 THEN 'due_date'
        WHEN 5 THEN 'insert_installment'
        WHEN 6 THEN 'delete_installment'
        WHEN 7 THEN 'grace_on_interest'
        WHEN 8 THEN 'grace_on_principal'
        WHEN 9 THEN 'extend_repayment_period'
        WHEN 10 THEN 'interest_rate_from_installment'
        WHEN 11 THEN 'interest_pause'
        ELSE 'other'
    END AS term_type_label,
    v.applicable_date,
    v.decimal_value,
    v.date_value,
    v.is_specific_to_installment,
    v.is_active
FROM m_loan_term_variations v
JOIN m_loan l ON l.id = v.loan_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY v.loan_id, v.applicable_date DESC, v.id DESC
LIMIT $2
