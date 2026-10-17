-- Non-analytical assistant turns share the existing durable job lifecycle.
ALTER TABLE job_responses DROP CONSTRAINT job_responses_kind_check;
ALTER TABLE job_responses ADD CONSTRAINT job_responses_kind_check
    CHECK (kind IN ('analysis', 'skipped', 'limitation', 'conversation'));
