-- Deviation build-order §6.5 (FIN-30): completeness_reason 'no_capability_matched'
-- conflated genuine out-of-scope requests with our own retrieval failures. Owner
-- decision 2026-09-20: split now — planner.rs emits 'out_of_scope' (no lexical
-- candidate) vs 'retrieval_miss' (candidate/index skew, our failure).
--
-- Historical rows cannot be reclassified from stored data (the original request
-- context is gone). Map the old value to the non-blaming default 'retrieval_miss'
-- rather than fabricate an 'out_of_scope' claim (I4/I5: no false certainty). New
-- jobs get the real split from the planner. Full out-of-scope detection is only
-- correct once L2 semantic retrieval lands (FIN-42). completeness_reason is free
-- TEXT (no CHECK), so this is a data backfill only.
UPDATE chat_jobs
   SET completeness_reason = 'retrieval_miss'
 WHERE completeness_reason = 'no_capability_matched';

UPDATE job_responses
   SET completeness_reason = 'retrieval_miss'
 WHERE completeness_reason = 'no_capability_matched';
