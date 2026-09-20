-- Deviation build-order §6.4 (FIN-29): Cancelled/Expired jobs were recorded with
-- outcome='OperationalFailure', a temporary placeholder (engine.md line 193). A
-- cancel is an abort (K3) and an expiry is a TTL stop — neither is an operational
-- failure nor an analysis result (I4: Abandoned ≠ Failed). Owner decision
-- 2026-09-20: direction (a) — add 'Cancelled'/'Expired' to the outcome vocabulary,
-- mirroring lifecycle.

-- 1. Widen the outcome CHECK to admit the two new terminal outcomes.
ALTER TABLE chat_jobs DROP CONSTRAINT chat_jobs_outcome_check;
ALTER TABLE chat_jobs ADD CONSTRAINT chat_jobs_outcome_check
    CHECK (outcome IN ('Answered', 'Empty', 'NotFound', 'Unsupported',
                       'BlockedByPolicy', 'Invalid', 'OperationalFailure',
                       'SkippedByUser', 'Cancelled', 'Expired'));

-- 2. Backfill the historical rows recorded under the placeholder. outcome mirrors
--    lifecycle for these terminal states; chat_jobs_terminal_has_outcome stays
--    satisfied (outcome is still non-null).
UPDATE chat_jobs
   SET outcome = lifecycle
 WHERE lifecycle IN ('Cancelled', 'Expired')
   AND outcome = 'OperationalFailure';
