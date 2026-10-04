use anyhow::{Context, ensure};
use foundation::{error::ApiError, state::Foundation};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    model::{IndexDocument, Run, RunView},
    repository::{self, AdmissionFields},
};
use crate::catalog::{self, Catalog, Checked};

/// Owns a dedicated PostgreSQL session lock until execution finishes. Dropping
/// it after process failure closes the session, allowing the next admission to
/// mark the interrupted record Abandoned under the same lock.
pub struct Admission {
    run: Run,
    checked: Checked,
    lock: PgConnection,
}

impl Admission {
    pub fn run(&self) -> &Run {
        &self.run
    }

    pub async fn execute(self, foundation: &Foundation) -> anyhow::Result<Run> {
        let Admission {
            run,
            checked,
            mut lock,
        } = self;
        let pool = foundation.app_db().pool();
        let operation = async {
            repository::mark_running(pool, run.id).await?;
            let documents = documents(&checked.catalog);
            ensure!(
                documents.len() == (run.capability_count + run.query_count) as usize,
                "validated catalog has empty retrieval text"
            );
            let (vectors, model) = if run.rebuild_embeddings && !documents.is_empty() {
                let client = foundation::embedding::EmbeddingClient::new(foundation.config())
                    .context("EMBEDDING_UNAVAILABLE")?;
                ensure!(client.available(), "EMBEDDING_UNAVAILABLE");
                ensure!(
                    client.dimensions() == 1024,
                    "EMBEDDING_CONFIGURATION_INVALID"
                );
                let mut output = Vec::with_capacity(documents.len());
                for batch in documents.chunks(32) {
                    let texts: Vec<String> = batch
                        .iter()
                        .map(|item| item.retrieval_text.clone())
                        .collect();
                    let vectors = client
                        .embed(&texts, foundation::embedding::InputKind::Document)
                        .await
                        .context("EMBEDDING_PROVIDER_ERROR")?;
                    for vector in &vectors {
                        ensure!(
                            vector.len() == client.dimensions()
                                && vector.iter().all(|v| v.is_finite()),
                            "EMBEDDING_RESPONSE_INVALID"
                        );
                    }
                    output.extend(vectors);
                    repository::progress(pool, run.id, output.len()).await?;
                }
                (
                    Some(output),
                    Some((
                        client.model().to_owned(),
                        client.dimensions(),
                        client.document_input_type().to_owned(),
                    )),
                )
            } else {
                (None, None)
            };
            repository::assert_lock_alive(&mut lock).await?;
            repository::publish(
                pool,
                &run,
                &documents,
                vectors.as_deref(),
                model
                    .as_ref()
                    .map(|(name, dims, input)| (name.as_str(), *dims, input.as_str())),
            )
            .await
        }
        .await;
        let result = match operation {
            Ok(finished) => Ok(finished),
            Err(error) => {
                let code = classify_error(&error);
                if let Err(persist_error) = repository::fail(pool, run.id, code).await {
                    tracing::error!(run_id=%run.id, error=%persist_error,
                        "failed to persist catalog reindex failure");
                }
                Err(error)
            }
        };
        if let Err(error) = repository::release(lock).await {
            tracing::error!(run_id=%run.id, error=%error, "catalog lock release failed");
        }
        result
    }
}

fn classify_error(error: &anyhow::Error) -> &'static str {
    let message = format!("{error:#}");
    if message.contains("EMBEDDING_UNAVAILABLE") {
        "EMBEDDING_UNAVAILABLE"
    } else if message.contains("EMBEDDING_CONFIGURATION_INVALID") {
        "EMBEDDING_CONFIGURATION_INVALID"
    } else if message.contains("EMBEDDING_RESPONSE_INVALID") {
        "EMBEDDING_RESPONSE_INVALID"
    } else if message.contains("EMBEDDING_PROVIDER_ERROR") {
        "EMBEDDING_PROVIDER_ERROR"
    } else {
        "CATALOG_REINDEX_FAILED"
    }
}

/// Shared admission API for HTTP and CLI. `checked` must be the freshly
/// probed disk catalog; a caller may pass a pinned process catalog to expose
/// the restart boundary without changing what workers currently execute.
pub async fn admit(
    foundation: &Foundation,
    checked: Checked,
    running: Option<&Catalog>,
    rebuild_embeddings: bool,
    owner_user_id: Option<Uuid>,
) -> Result<Admission, ApiError> {
    if checked.report.errors() > 0 {
        let mut codes: Vec<&str> = checked
            .report
            .findings
            .iter()
            .filter(|finding| finding.severity == catalog::Severity::Error)
            .map(|finding| finding.check.as_str())
            .collect();
        codes.sort_unstable();
        codes.dedup();
        return Err(ApiError::Unprocessable(format!(
            "Catalog validation failed: {} errors; finding codes: {}",
            checked.report.errors(),
            codes.join(",")
        )));
    }
    let missing_text = checked.catalog.capabilities.len() + checked.catalog.queries.len()
        - documents(&checked.catalog).len();
    if missing_text > 0 {
        return Err(ApiError::Unprocessable(format!(
            "Catalog validation failed: {missing_text} errors; finding codes: retrieval_text_empty"
        )));
    }
    let lock = repository::acquire(&foundation.config().app_database_url)
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::Conflict("Catalog maintenance is already running".into()))?;
    let resolver_shapes = checked
        .catalog
        .datasets
        .iter()
        .flat_map(|dataset| {
            dataset
                .entry
                .shapes
                .iter()
                .map(move |shape| (&dataset.entry.id, &shape.id))
        })
        .filter(|(dataset_id, shape_id)| {
            checked.catalog.resolver_for(dataset_id, shape_id).is_some()
        })
        .count();
    let fields = AdmissionFields {
        hash: &checked.catalog.content_hash,
        running_hash: running.map(|catalog| catalog.content_hash.as_str()),
        rebuild: rebuild_embeddings,
        owner: owner_user_id,
        capabilities: checked.catalog.capabilities.len(),
        queries: checked.catalog.queries.len(),
        datasets: checked.catalog.datasets.len(),
        resolver_shapes,
        warnings: checked.report.warnings(),
        document_count: checked.catalog.document_count(),
    };
    let run = repository::admit(foundation.app_db().pool(), fields)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Admission { run, checked, lock })
}

pub async fn checked_current(foundation: &Foundation) -> Result<Checked, ApiError> {
    catalog::check(foundation, true).await.map_err(|error| {
        tracing::error!(error=%error, "catalog reindex validation unavailable");
        ApiError::Internal(anyhow::anyhow!("catalog validation unavailable"))
    })
}

pub async fn view(
    foundation: &Foundation,
    running: &Catalog,
    run: Run,
) -> Result<RunView, ApiError> {
    let running_catalog_version_id =
        repository::version_id(foundation.app_db().pool(), &running.content_hash)
            .await
            .map_err(|error| ApiError::Internal(error.into()))?;
    let restart_required = run.status == "Completed" && run.content_hash != running.content_hash;
    let indexed_content_hash = (run.status == "Completed").then(|| run.content_hash.clone());
    let indexed_catalog_version_id = if run.status == "Completed" {
        run.catalog_version_id
    } else {
        None
    };
    let (published_lexical_row_count, published_embedded_row_count) =
        if let Some(version) = run.catalog_version_id {
            repository::coverage(foundation.app_db().pool(), version)
                .await
                .map_err(|error| ApiError::Internal(error.into()))?
        } else {
            (0, 0)
        };
    Ok(RunView {
        run,
        indexed_content_hash,
        indexed_catalog_version_id,
        running_catalog_content_hash: running.content_hash.clone(),
        running_catalog_version_id,
        published_lexical_row_count,
        published_embedded_row_count,
        restart_required,
    })
}

pub async fn get(
    foundation: &Foundation,
    running: &Catalog,
    id: Uuid,
) -> Result<RunView, ApiError> {
    let run = repository::get(foundation.app_db().pool(), id)
        .await
        .map_err(|error| ApiError::Internal(error.into()))?
        .ok_or(ApiError::NotFound)?;
    view(foundation, running, run).await
}

pub async fn recent(foundation: &Foundation, running: &Catalog) -> Result<Vec<RunView>, ApiError> {
    let rows = repository::recent(foundation.app_db().pool())
        .await
        .map_err(|error| ApiError::Internal(error.into()))?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        views.push(view(foundation, running, row).await?);
    }
    Ok(views)
}

fn documents(catalog: &Catalog) -> Vec<IndexDocument> {
    let mut result = Vec::with_capacity(catalog.capabilities.len() + catalog.queries.len());
    for loaded in &catalog.capabilities {
        let item = &loaded.entry;
        let mut text = Vec::new();
        if let Some(name) = &item.display_name {
            text.push(name.clone());
        }
        if let Some(description) = &item.description {
            text.push(description.clone());
        }
        text.extend(item.supported_intents.iter().cloned());
        text.extend(item.examples.iter().cloned());
        let retrieval_text = text.join("\n");
        if retrieval_text.trim().is_empty() {
            continue;
        }
        result.push(IndexDocument {
            source_type: "capability",
            source_id: item.id.clone(),
            source_path: loaded.path.clone(),
            title: item.display_name.clone(),
            retrieval_text,
            metadata: serde_json::json!({"domain":item.domain,"status":item.status,
                "query_id":item.query_id}),
        });
    }
    for loaded in &catalog.queries {
        let item = &loaded.entry;
        result.push(IndexDocument {
            source_type: "query",
            source_id: item.id.clone(),
            source_path: loaded.path.clone(),
            title: Some(item.id.clone()),
            retrieval_text: format!(
                "{} parameter: {}",
                item.id,
                item.parameters
                    .iter()
                    .map(|parameter| parameter.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            metadata: serde_json::json!({"sql_file":item.sql_file,"timeout_ms":item.timeout_ms}),
        });
    }
    result
}
