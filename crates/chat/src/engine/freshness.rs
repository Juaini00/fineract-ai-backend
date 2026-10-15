//! D14 / XR-ASOF — freshness hand-off for analyses that depend on a batch.
//!
//! A capability that declares `freshness:` is only `Complete` when account-level
//! evidence proves the batch closed every in-scope account through the COB
//! date. A successful batch run NEVER proves that by itself: it is reported as
//! run evidence beside the verdict, never used as the verdict.
//!
//! Rules, in order (the first that matches wins; all deterministic):
//!
//! 1. batch probe failed                      → `Unknown` `freshness_probe_failed`
//! 2. batch job not registered                → `Unknown` `batch_job_not_registered`
//! 3. no COB date row                         → `Unknown` `cob_date_unavailable`
//! 4. coverage probe failed / no row          → `Unknown` `coverage_probe_failed`
//! 5. some in-scope loan behind the COB date  → `Partial` `batch_coverage_incomplete`
//! 6. no batch run observed at all            → `Unknown` `batch_run_not_observed`
//! 7. latest run status not `success`/`failed`→ `Unknown` `batch_latest_status_unrecognized`
//! 8. latest batch run failed                 → `Partial` `batch_latest_run_failed`
//! 9. latest run `success` AND nothing behind → `Complete`
//!
//! `Complete` therefore needs BOTH: the required batch observed to have run and
//! its latest run recorded as `success` (necessary), AND account-level coverage
//! (sufficient together). Nothing is guessed from an absent or unfamiliar status.
//!
//! An answer with no rows can never be `Partial` (engine.md forbids
//! `Empty` + `Partial`), so [`Claim::for_rows`] raises it to `Unknown`.

use chrono::NaiveDate;
use foundation::db::FineractDb;
use serde_json::{Map, Value, json};

use crate::engine::executor;
use crate::engine::planner::Bound;

pub const FRESHNESS_PROBE_FAILED: &str = "freshness_probe_failed";
pub const BATCH_JOB_NOT_REGISTERED: &str = "batch_job_not_registered";
pub const COB_DATE_UNAVAILABLE: &str = "cob_date_unavailable";
pub const COVERAGE_PROBE_FAILED: &str = "coverage_probe_failed";
pub const BATCH_COVERAGE_INCOMPLETE: &str = "batch_coverage_incomplete";
pub const BATCH_LATEST_RUN_FAILED: &str = "batch_latest_run_failed";
pub const BATCH_RUN_NOT_OBSERVED: &str = "batch_run_not_observed";
pub const BATCH_LATEST_STATUS_UNRECOGNIZED: &str = "batch_latest_status_unrecognized";

/// `block_id` of the response block that states the freshness claim.
pub const FRESHNESS_BLOCK_ID: &str = "freshness";

/// An approved probe query, ready to run.
#[derive(Debug, Clone)]
pub struct ProbePlan {
    pub query_id: String,
    pub sql: String,
    pub timeout_ms: u64,
}

/// What a capability's `freshness:` declaration resolved to at plan time.
#[derive(Debug, Clone)]
pub struct FreshnessPlan {
    pub batch_job: String,
    pub batch: ProbePlan,
    pub coverage: ProbePlan,
}

/// Outcome of one probe: a row, no row, or the query failing.
#[derive(Debug)]
pub enum Probe {
    Row(Map<String, Value>),
    NoRow,
    Failed(&'static str),
}

/// The freshness verdict plus the facts it was computed from.
#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    pub completeness: &'static str,
    /// `None` only for `Complete`.
    pub reason: Option<&'static str>,
    pub facts: Value,
    /// Source queries actually sent by [`gather`] (batch probe, plus the
    /// coverage probe when a COB date was read). They are charged to the job's
    /// absolute `query_count` budget in T4, beside the primary query.
    pub queries_issued: i64,
}

impl Claim {
    /// Engine rule: an answer with zero rows may be `Complete` or `Unknown`,
    /// never `Partial`.
    pub fn for_rows(mut self, row_count: usize) -> Self {
        if row_count == 0 && self.completeness == "Partial" {
            self.completeness = "Unknown";
        }
        self
    }
}

/// Run both probes and compute the claim. Runs outside any commit
/// transaction (I1), like the analysis query itself.
pub async fn gather(fineract: &FineractDb, plan: &FreshnessPlan, office_ids: &[i64]) -> Claim {
    let mut queries_issued = 1;
    let batch = probe(
        fineract,
        &plan.batch,
        &[Bound::Text(plan.batch_job.clone())],
    )
    .await;

    // The coverage probe needs the COB date the batch probe read; without it
    // the question "behind what?" has no answer, so it is not asked.
    let cob_date = match &batch {
        Probe::Row(row) => date_field(row, "cob_date"),
        _ => None,
    };
    let coverage = match cob_date {
        Some(cob_date) => {
            queries_issued += 1;
            probe(
                fineract,
                &plan.coverage,
                &[Bound::OfficeIds(office_ids.to_vec()), Bound::Date(cob_date)],
            )
            .await
        }
        None => Probe::NoRow,
    };

    Claim {
        queries_issued,
        ..assess(&plan.batch_job, &batch, &coverage)
    }
}

async fn probe(fineract: &FineractDb, plan: &ProbePlan, parameters: &[Bound]) -> Probe {
    match executor::run(fineract, &plan.sql, parameters, plan.timeout_ms).await {
        Ok(executed) => executed
            .rows
            .into_iter()
            .next()
            .map_or(Probe::NoRow, Probe::Row),
        Err(error) => Probe::Failed(error.failure_code()),
    }
}

/// Pure verdict over the probe results (see module docs for the rules).
pub fn assess(batch_job: &str, batch: &Probe, coverage: &Probe) -> Claim {
    let batch_row = match batch {
        Probe::Row(row) => row,
        Probe::NoRow => return unknown(batch_job, BATCH_JOB_NOT_REGISTERED, json!({})),
        Probe::Failed(code) => {
            return unknown(
                batch_job,
                FRESHNESS_PROBE_FAILED,
                json!({ "probe_failure": code }),
            );
        }
    };

    let batch_facts = json!({
        "job_id": batch_row.get("job_id"),
        "job_is_active": batch_row.get("job_is_active"),
        "latest_run": {
            "run_id": batch_row.get("latest_run_id"),
            "start_time": batch_row.get("latest_run_start_time"),
            "status": batch_row.get("latest_run_status"),
        },
        "latest_success": {
            "run_id": batch_row.get("latest_success_run_id"),
            "start_time": batch_row.get("latest_success_start_time"),
        },
        "cob_date": batch_row.get("cob_date"),
        "business_date": batch_row.get("business_date"),
    });

    if date_field(batch_row, "cob_date").is_none() {
        return unknown(batch_job, COB_DATE_UNAVAILABLE, batch_facts);
    }

    let coverage_row = match coverage {
        Probe::Row(row) => row,
        Probe::NoRow => return unknown(batch_job, COVERAGE_PROBE_FAILED, batch_facts),
        Probe::Failed(code) => {
            let mut facts = batch_facts;
            facts["probe_failure"] = json!(code);
            return unknown(batch_job, COVERAGE_PROBE_FAILED, facts);
        }
    };

    let in_scope = int_field(coverage_row, "in_scope_loans");
    let behind = int_field(coverage_row, "loans_behind_cob");
    let (Some(in_scope), Some(behind)) = (in_scope, behind) else {
        return unknown(batch_job, COVERAGE_PROBE_FAILED, batch_facts);
    };

    let mut facts = batch_facts;
    facts["coverage"] = json!({
        "in_scope_loans": in_scope,
        "loans_behind_cob": behind,
        "oldest_last_closed_business_date": coverage_row.get("oldest_last_closed_business_date"),
    });

    // The required batch must be observed to have run, and its latest run must be
    // a status this code understands: an absent or unfamiliar status is never
    // read as success (XR-ASOF — no source success guess).
    let latest_status = batch_row.get("latest_run_status").and_then(Value::as_str);
    let run_observed = !batch_row.get("latest_run_id").is_none_or(Value::is_null);
    let (completeness, reason) = if behind > 0 {
        ("Partial", Some(BATCH_COVERAGE_INCOMPLETE))
    } else if !run_observed || latest_status.is_none() {
        ("Unknown", Some(BATCH_RUN_NOT_OBSERVED))
    } else {
        match latest_status {
            Some("success") => ("Complete", None),
            Some("failed") => ("Partial", Some(BATCH_LATEST_RUN_FAILED)),
            _ => ("Unknown", Some(BATCH_LATEST_STATUS_UNRECOGNIZED)),
        }
    };

    Claim {
        completeness,
        reason,
        facts: with_job(batch_job, facts),
        queries_issued: 0,
    }
}

fn unknown(batch_job: &str, reason: &'static str, facts: Value) -> Claim {
    Claim {
        completeness: "Unknown",
        reason: Some(reason),
        facts: with_job(batch_job, facts),
        queries_issued: 0,
    }
}

fn with_job(batch_job: &str, mut facts: Value) -> Value {
    facts["batch_job"] = json!(batch_job);
    facts
}

fn date_field(row: &Map<String, Value>, name: &str) -> Option<NaiveDate> {
    row.get(name)
        .and_then(Value::as_str)
        .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
}

fn int_field(row: &Map<String, Value>, name: &str) -> Option<i64> {
    row.get(name).and_then(Value::as_i64)
}

/// Prose for the response block. Numbers stay in the structured `facts`; the
/// sentence carries none, so the D3 numeral check has nothing to ground.
pub fn explain(claim: &Claim) -> &'static str {
    match claim.reason {
        None => {
            "Account-level evidence shows every in-scope loan closed through the COB date. \
             A successful batch run alone would not have proved this."
        }
        Some(BATCH_COVERAGE_INCOMPLETE) => {
            "Some in-scope loans have not been closed by the batch through the COB date, so the \
             arrears snapshot may not reflect the latest business day. A successful batch run \
             in the history does not prove these loans were covered."
        }
        Some(BATCH_LATEST_RUN_FAILED) => {
            "Every in-scope loan is closed through the COB date, but the latest batch run \
             failed, so the evidence conflicts and the snapshot is reported as partial."
        }
        Some(BATCH_RUN_NOT_OBSERVED) => {
            "The required batch has no recorded run, so even with no loan behind the COB date \
             it cannot be confirmed that the batch produced the current state."
        }
        Some(BATCH_LATEST_STATUS_UNRECOGNIZED) => {
            "The latest run of the required batch has a status that is neither success nor \
             failure, so freshness is not assumed."
        }
        Some(BATCH_JOB_NOT_REGISTERED) => {
            "The batch job this analysis depends on is not registered in the scheduler, so \
             freshness cannot be established."
        }
        Some(COB_DATE_UNAVAILABLE) => {
            "No close-of-business date is recorded, so there is no business period to measure \
             coverage against."
        }
        Some(_) => {
            "The freshness evidence could not be read, so it is unknown whether the batch \
             covered the in-scope loans."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(status: &str, cob: Option<&str>) -> Probe {
        let mut row = Map::new();
        row.insert("job_id".into(), json!(34));
        row.insert("cob_date".into(), cob.map_or(Value::Null, |d| json!(d)));
        row.insert("latest_run_id".into(), json!(307177));
        row.insert("latest_run_status".into(), json!(status));
        Probe::Row(row)
    }

    fn coverage(in_scope: i64, behind: i64) -> Probe {
        let mut row = Map::new();
        row.insert("in_scope_loans".into(), json!(in_scope));
        row.insert("loans_behind_cob".into(), json!(behind));
        Probe::Row(row)
    }

    /// The central D14 invariant: a successful batch run does not make the
    /// analysis Complete while any in-scope loan is behind the COB date.
    #[test]
    fn success_alone_never_proves_coverage() {
        let claim = assess(
            "Loan COB",
            &batch("success", Some("2026-09-07")),
            &coverage(87, 3),
        );
        assert_eq!(claim.completeness, "Partial");
        assert_eq!(claim.reason, Some(BATCH_COVERAGE_INCOMPLETE));
        assert_eq!(claim.facts["coverage"]["loans_behind_cob"], 3);
    }

    #[test]
    fn complete_only_with_full_coverage_and_a_non_failed_latest_run() {
        let claim = assess(
            "Loan COB",
            &batch("success", Some("2026-09-07")),
            &coverage(87, 0),
        );
        assert_eq!((claim.completeness, claim.reason), ("Complete", None));

        let conflict = assess(
            "Loan COB",
            &batch("failed", Some("2026-09-07")),
            &coverage(87, 0),
        );
        assert_eq!(conflict.reason, Some(BATCH_LATEST_RUN_FAILED));
    }

    /// XR-ASOF: no loan behind the COB date is NOT enough — an absent run or an
    /// unfamiliar status is Unknown, never a guessed success.
    #[test]
    fn complete_needs_an_observed_successful_batch_not_just_zero_loans_behind() {
        let covered = coverage(87, 0);
        let mut never_ran = Map::new();
        never_ran.insert("cob_date".into(), json!("2026-09-07"));
        let claim = assess("Loan COB", &Probe::Row(never_ran), &covered);
        assert_eq!(
            (claim.completeness, claim.reason),
            ("Unknown", Some(BATCH_RUN_NOT_OBSERVED))
        );

        let claim = assess(
            "Loan COB",
            &batch("running", Some("2026-09-07")),
            &coverage(87, 0),
        );
        assert_eq!(
            (claim.completeness, claim.reason),
            ("Unknown", Some(BATCH_LATEST_STATUS_UNRECOGNIZED))
        );

        let mut absent = Map::new();
        absent.insert("cob_date".into(), json!("2026-09-07"));
        absent.insert("latest_run_id".into(), json!(1));
        let claim = assess("Loan COB", &Probe::Row(absent), &coverage(0, 0));
        assert_eq!(claim.completeness, "Unknown");
    }

    #[test]
    fn unavailable_evidence_is_unknown_never_complete() {
        let unregistered = assess("Loan COB", &Probe::NoRow, &Probe::NoRow);
        assert_eq!(unregistered.reason, Some(BATCH_JOB_NOT_REGISTERED));
        let no_cob = assess("Loan COB", &batch("success", None), &Probe::NoRow);
        assert_eq!(no_cob.reason, Some(COB_DATE_UNAVAILABLE));
        let failed = assess(
            "Loan COB",
            &Probe::Failed("source_query_timeout"),
            &Probe::NoRow,
        );
        assert_eq!(failed.reason, Some(FRESHNESS_PROBE_FAILED));
        let no_coverage = assess(
            "Loan COB",
            &batch("success", Some("2026-09-07")),
            &Probe::NoRow,
        );
        assert_eq!(
            (no_coverage.completeness, no_coverage.reason),
            ("Unknown", Some(COVERAGE_PROBE_FAILED))
        );
    }

    /// engine.md: `Empty` may be `Complete` or `Unknown`, never `Partial`.
    #[test]
    fn empty_answers_never_stay_partial() {
        let partial = assess(
            "Loan COB",
            &batch("success", Some("2026-09-07")),
            &coverage(87, 87),
        );
        assert_eq!(partial.clone().for_rows(0).completeness, "Unknown");
        assert_eq!(partial.for_rows(2).completeness, "Partial");
    }
}
