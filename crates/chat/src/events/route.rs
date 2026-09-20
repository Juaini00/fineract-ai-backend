//! `GET /chat/jobs/{job_id}/events` — SSE.
//!
//! Bearer dibaca dari header, tidak pernah dari URL (sse.md): URL muncul di log
//! proxy, riwayat browser, dan Referer.

use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    Extension, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
    routing::get,
};
use foundation::{AuthUser, error::ApiError, state::Foundation};
use serde::Deserialize;
use uuid::Uuid;

use crate::events::{Hub, repository::JobEvent, service};

/// Header standar SSE untuk melanjutkan dari posisi terakhir.
const LAST_EVENT_ID: &str = "Last-Event-ID";

pub fn router() -> Router<Foundation> {
    Router::new().route("/chat/jobs/{job_id}/events", get(events))
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// Alternatif `Last-Event-ID` untuk klien fetch yang tidak dapat mengatur
    /// header pada reconnect. Header menang bila keduanya ada.
    #[serde(default)]
    cursor: Option<i64>,
}

async fn events(
    State(foundation): State<Foundation>,
    Extension(hub): Extension<Arc<Hub>>,
    user: AuthUser,
    Path(job_id): Path<Uuid>,
    Query(query): Query<EventsQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let cursor = last_event_id(&headers)?.or(query.cursor).unwrap_or(0);

    let stream = service::open(&foundation, &hub, job_id, user.user_id, cursor).await?;
    let comment_interval = foundation.config().sse_transport_comment_interval_secs;

    // State unfold ADALAH stream-nya: cursor hidup di satu tempat, sehingga
    // tidak ada salinan kedua yang dapat menyimpang darinya.
    let frames = futures::stream::unfold(stream, |mut stream| async move {
        stream
            .next()
            .await
            .map(|event| (Ok(frame(&event)), stream))
    });

    Ok(Sse::new(frames).keep_alive(
        KeepAlive::new()
            // Komentar keep-alive menjaga transport hidup. Ia BUKAN bukti worker
            // masih bekerja — itu tugas lease (sse.md).
            .interval(Duration::from_secs(comment_interval))
            .text("keep-alive"),
    ))
}

/// Susun satu frame SSE. `id` adalah `sequence`, sehingga reconnect dengan
/// `Last-Event-ID` melanjutkan tepat sesudahnya.
fn frame(event: &JobEvent) -> Event {
    Event::default()
        .id(event.sequence.to_string())
        .event(event.event_type.clone())
        .json_data(envelope(event))
        .unwrap_or_else(|_| Event::default().event("job.notice").data("{}"))
}

/// Envelope durable (sse.md §Event vocabulary).
///
/// `payload_json` disebar ke level atas supaya klien tidak perlu tahu bahwa ia
/// tersimpan sebagai satu kolom; referensi bertipe tetap menjadi field
/// tersendiri karena ia selalu tersedia berapa pun ambang inline.
fn envelope(event: &JobEvent) -> serde_json::Value {
    let mut body = serde_json::json!({
        "schema_version": event.schema_version,
        "sequence": event.sequence,
        "event": event.event_type,
        "occurred_at": event.occurred_at,
        "plan_version": event.plan_version,
        "node_id": event.node_id,
        "node_attempt": event.node_attempt,
        "clarification_id": event.clarification_id,
        "clarification_revision": event.clarification_revision,
        "response_version": event.response_version,
        "payload_truncated": event.payload_truncated,
    });

    if let (Some(object), Some(payload)) = (
        body.as_object_mut(),
        event.payload_json.as_ref().and_then(|value| value.as_object()),
    ) {
        for (key, value) in payload {
            object.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }

    body
}

/// `Last-Event-ID` yang tidak berupa angka adalah kesalahan klien, bukan alasan
/// diam-diam mengulang dari nol — mengulang dari nol akan mengirim ulang seluruh
/// riwayat seolah itu kemajuan baru.
fn last_event_id(headers: &HeaderMap) -> Result<Option<i64>, ApiError> {
    let Some(raw) = headers.get(LAST_EVENT_ID) else {
        return Ok(None);
    };

    raw.to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<i64>().map_err(|_| {
                ApiError::Unprocessable(format!("{LAST_EVENT_ID} must be an event sequence number"))
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn event(payload: Option<serde_json::Value>) -> JobEvent {
        JobEvent {
            sequence: 42,
            schema_version: 1,
            event_type: "job.phase_changed".into(),
            occurred_at: Utc::now(),
            plan_version: Some(2),
            node_id: None,
            node_attempt: None,
            clarification_id: None,
            clarification_revision: None,
            response_version: None,
            payload_json: payload,
            payload_truncated: false,
        }
    }

    fn headers_with(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(LAST_EVENT_ID, value.parse().unwrap());
        headers
    }

    #[test]
    fn envelope_carries_the_durable_identity_of_the_event() {
        let body = envelope(&event(Some(serde_json::json!({ "phase": "planning" }))));

        assert_eq!(body["schema_version"], 1);
        assert_eq!(body["sequence"], 42);
        assert_eq!(body["event"], "job.phase_changed");
        assert_eq!(body["plan_version"], 2);
        assert_eq!(body["phase"], "planning");
    }

    #[test]
    fn payload_never_overwrites_the_envelope() {
        // Payload yang memuat `sequence` tidak boleh menggeser sequence asli:
        // cursor klien dihitung dari field itu.
        let body = envelope(&event(Some(serde_json::json!({ "sequence": 999 }))));
        assert_eq!(body["sequence"], 42);
    }

    #[test]
    fn missing_last_event_id_means_replay_from_the_beginning() {
        assert_eq!(last_event_id(&HeaderMap::new()).unwrap(), None);
    }

    #[test]
    fn numeric_last_event_id_is_accepted() {
        assert_eq!(last_event_id(&headers_with("41")).unwrap(), Some(41));
    }

    #[test]
    fn malformed_last_event_id_is_rejected_not_silently_reset() {
        assert!(last_event_id(&headers_with("abc")).is_err());
    }
}
