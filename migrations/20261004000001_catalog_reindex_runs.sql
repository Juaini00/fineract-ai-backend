-- Durable catalog maintenance state. The advisory lock is the cross-process
-- executor fence; these rows are evidence and must never imply active grants.
CREATE TABLE catalog_reindex_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    status TEXT NOT NULL CHECK (status IN ('Pending','Running','Completed','Failed','Abandoned')),
    rebuild_embeddings BOOLEAN NOT NULL,
    requested_by_user_id UUID,
    catalog_version_id UUID REFERENCES knowledge_catalog_versions(id) ON DELETE NO ACTION,
    content_hash TEXT NOT NULL,
    running_content_hash TEXT,
    capability_count INTEGER NOT NULL CHECK (capability_count >= 0),
    query_count INTEGER NOT NULL CHECK (query_count >= 0),
    dataset_definition_count INTEGER NOT NULL CHECK (dataset_definition_count >= 0),
    resolver_shape_count INTEGER NOT NULL CHECK (resolver_shape_count >= 0),
    lexical_row_count INTEGER NOT NULL DEFAULT 0 CHECK (lexical_row_count >= 0),
    embedded_row_count INTEGER NOT NULL DEFAULT 0 CHECK (embedded_row_count >= 0),
    processed_row_count INTEGER NOT NULL DEFAULT 0 CHECK (processed_row_count >= 0),
    finding_error_count INTEGER NOT NULL DEFAULT 0 CHECK (finding_error_count >= 0),
    finding_warning_count INTEGER NOT NULL DEFAULT 0 CHECK (finding_warning_count >= 0),
    error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    CONSTRAINT catalog_reindex_run_time CHECK (
      (status IN ('Pending','Running') AND finished_at IS NULL)
      OR (status IN ('Completed','Failed','Abandoned') AND finished_at IS NOT NULL)
    )
);

CREATE INDEX catalog_reindex_runs_recent_idx ON catalog_reindex_runs(created_at DESC, id DESC);
CREATE UNIQUE INDEX catalog_reindex_one_active_idx ON catalog_reindex_runs ((true))
  WHERE status IN ('Pending','Running');
