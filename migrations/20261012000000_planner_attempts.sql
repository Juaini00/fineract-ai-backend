-- Keep the repair allowance across lease recovery. Each key identifies one
-- immutable request/catalog/accepted-binding snapshot; the absolute call/token
-- budgets remain on the same chat_jobs row and never reset with a new key.
ALTER TABLE chat_jobs
    ADD COLUMN planner_call_counts JSONB NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(planner_call_counts) = 'object');
