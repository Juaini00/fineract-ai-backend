//! Kebijakan stream: cursor, replay, live, dan kapan berhenti.
//!
//! Yang dijaga di sini dan tidak boleh bergeser:
//!
//! - **Cursor bukan kredensial** (sse.md). Kepemilikan job diperiksa lebih dulu;
//!   cursor hanya menentukan dari mana replay dimulai.
//! - **Tidak ada penghilangan senyap.** Cursor yang mendahului riwayat, atau
//!   yang riwayatnya sudah hilang, menjadi kesalahan yang menyebut sebabnya —
//!   bukan stream yang mulai dari tengah.
//! - **Disconnect bukan cancel.** Stream berakhir hanya pada event terminal;
//!   tidak ada jalur di sini yang menyentuh lifecycle job.

use std::{collections::VecDeque, sync::Arc, time::Duration};

use foundation::{error::ApiError, state::Foundation};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    events::{
        Hub,
        repository::{self, JobEvent},
    },
    job::{repository::Job, service as job_service},
};

/// Event yang menutup stream: sesudahnya tidak akan ada event lain untuk job
/// ini, jadi klien berhenti menyambung ulang (sse.md aturan 6).
const TERMINAL_EVENTS: [&str; 4] = ["job.completed", "job.failed", "job.cancelled", "job.expired"];

/// Lifecycle terminal bersifat immutable (engine.md): sesudahnya tidak akan ada
/// event baru untuk job ini, berapa lama pun ditunggu.
const TERMINAL_LIFECYCLES: [&str; 4] = ["Completed", "Failed", "Cancelled", "Expired"];

pub fn is_terminal(event_type: &str) -> bool {
    TERMINAL_EVENTS.contains(&event_type)
}

fn is_settled(lifecycle: &str) -> bool {
    TERMINAL_LIFECYCLES.contains(&lifecycle)
}

/// Sumber frame untuk satu koneksi.
pub struct Stream {
    foundation: Foundation,
    job_id: Uuid,
    cursor: i64,
    /// Frame yang sudah dibaca dari PostgreSQL tetapi belum dikirim. Panjangnya
    /// dibatasi `SSE_OUTGOING_BUFFER_FRAMES`.
    pending: VecDeque<JobEvent>,
    notifications: broadcast::Receiver<Uuid>,
    poll_interval: Duration,
    poll_interval_max: Duration,
    backoff: Duration,
    buffer_frames: i64,
    finished: bool,
}

/// Siapkan stream: periksa kepemilikan, validasi cursor, lalu mulai dari sana.
pub async fn open(
    foundation: &Foundation,
    hub: &Arc<Hub>,
    job_id: Uuid,
    owner_user_id: Uuid,
    cursor: i64,
) -> Result<Stream, ApiError> {
    // Otorisasi lebih dulu, dan tidak pernah dari cursor: job milik orang lain
    // tampak `NotFound` persis seperti pada endpoint lain.
    let job = job_service::owned(foundation, job_id, owner_user_id).await?;

    validate_cursor(foundation, &job, cursor).await?;

    let config = foundation.config();

    Ok(Stream {
        foundation: foundation.clone(),
        job_id,
        cursor,
        pending: VecDeque::new(),
        // Berlangganan SEBELUM pembacaan pertama: notifikasi yang tiba di antara
        // pembacaan dan langganan akan hilang, dan itu adalah lubang yang
        // fallback polling harus menutup — bukan lubang yang boleh ada.
        notifications: hub.subscribe(),
        poll_interval: Duration::from_secs(config.sse_fallback_poll_interval_secs),
        poll_interval_max: Duration::from_secs(config.sse_fallback_poll_max_interval_secs),
        backoff: Duration::from_secs(config.sse_fallback_poll_interval_secs),
        buffer_frames: config.sse_outgoing_buffer_frames as i64,
        // Job yang sudah settled dan cursor yang sudah mencapai event
        // terakhirnya: tidak akan pernah ada event lain. Membiarkan stream
        // menunggu akan membuat klien menyambung ulang selamanya ke job yang
        // sudah selesai — persis yang sse.md aturan 6 larang.
        finished: is_settled(&job.lifecycle) && cursor >= job.last_event_sequence,
    })
}

async fn validate_cursor(foundation: &Foundation, job: &Job, cursor: i64) -> Result<(), ApiError> {
    if cursor < 0 {
        return Err(ApiError::Unprocessable(
            "Event cursor cannot be negative".to_string(),
        ));
    }

    if cursor > job.last_event_sequence {
        return Err(ApiError::Conflict(format!(
            "Event cursor {cursor} is ahead of this job (last committed sequence is {}); \
             fetch a fresh job snapshot before subscribing",
            job.last_event_sequence
        )));
    }

    // Cursor 0 berarti "dari awal" dan selalu sah. Selain itu, riwayat yang
    // dibutuhkan wajib masih ada: bila event terlama sudah melewati cursor,
    // ada bagian yang tidak akan pernah terkirim.
    if cursor > 0 {
        let earliest = repository::earliest(foundation.app_db().pool(), job.id)
            .await
            .map_err(anyhow::Error::from)?;

        if let Some(earliest) = earliest
            && earliest > cursor + 1
        {
            return Err(ApiError::Conflict(format!(
                "Event history before sequence {earliest} is no longer retained; \
                 fetch a fresh job snapshot before subscribing"
            )));
        }
    }

    Ok(())
}

impl Stream {
    pub fn cursor(&self) -> i64 {
        self.cursor
    }

    /// Event berikutnya, atau `None` bila stream berakhir.
    ///
    /// Urutannya: kirim yang sudah dibaca → baca PostgreSQL → tunggu notifikasi
    /// **atau** batas polling, mana yang lebih dulu. Menunggu notifikasi saja
    /// akan membuat stream menggantung selamanya ketika notifikasi hilang.
    pub async fn next(&mut self) -> Option<JobEvent> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                self.cursor = event.sequence;
                if is_terminal(&event.event_type) {
                    self.finished = true;
                }
                return Some(event);
            }

            if self.finished {
                return None;
            }

            if self.fill().await {
                continue;
            }

            self.idle().await;
        }
    }

    /// Baca event baru. `true` bila ada sesuatu untuk dikirim.
    async fn fill(&mut self) -> bool {
        match repository::after(
            self.foundation.app_db().pool(),
            self.job_id,
            self.cursor,
            self.buffer_frames,
        )
        .await
        {
            Ok(events) if !events.is_empty() => {
                // Ada kemajuan: kembalikan interval polling ke yang rapat.
                self.backoff = self.poll_interval;
                self.pending.extend(events);
                true
            }
            Ok(_) => false,
            Err(error) => {
                // Database bermasalah. Stream tidak ditutup — menutupnya membuat
                // klien menyimpulkan job berakhir, padahal yang gagal adalah
                // pembacaan. Ia mundur dan mencoba lagi.
                tracing::warn!(job_id = %self.job_id, error = %error, "pembacaan event gagal");
                self.grow_backoff();
                false
            }
        }
    }

    /// Tunggu notifikasi atau batas polling.
    async fn idle(&mut self) {
        let waited = tokio::time::timeout(self.backoff, self.wait_for_notification()).await;

        match waited {
            // Ada notifikasi: baca segera dan kembali ke interval rapat.
            Ok(()) => self.backoff = self.poll_interval,
            // Tidak ada notifikasi sama sekali — Redis mati, listener putus,
            // atau notifikasinya memang hilang. Tetap membaca PostgreSQL, dengan
            // interval yang melebar supaya job yang menganggur lama tidak
            // menjadi beban tetap.
            Err(_) => self.grow_backoff(),
        }
    }

    async fn wait_for_notification(&mut self) {
        loop {
            match self.notifications.recv().await {
                Ok(job_id) if job_id == self.job_id => return,
                Ok(_) => continue,
                // Tertinggal terlalu jauh: sinyal bangun yang hilang, BUKAN
                // event yang hilang. Perlakukan sebagai "ada yang baru" dan
                // biarkan PostgreSQL yang menentukan kebenarannya.
                Err(broadcast::error::RecvError::Lagged(_)) => return,
                Err(broadcast::error::RecvError::Closed) => {
                    // Hub berhenti (shutdown). Polling menjadi satu-satunya
                    // sumber; jangan sibuk berputar.
                    tokio::time::sleep(self.backoff).await;
                    return;
                }
            }
        }
    }

    fn grow_backoff(&mut self) {
        self.backoff = (self.backoff * 2).min(self.poll_interval_max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_settled_events_close_the_stream() {
        for event in ["job.completed", "job.failed", "job.cancelled", "job.expired"] {
            assert!(is_terminal(event), "{event} seharusnya menutup stream");
        }
    }

    #[test]
    fn settled_lifecycles_match_the_engine_matrix() {
        for lifecycle in ["Completed", "Failed", "Cancelled", "Expired"] {
            assert!(is_settled(lifecycle), "{lifecycle} terminal menurut engine.md");
        }
        // `WaitingForUser` adalah suspensi, bukan akhir: stream yang menutup di
        // sini membuat klien berhenti menyambung tepat saat jawabannya diterima.
        for lifecycle in ["Queued", "Running", "WaitingForUser", "Cancelling"] {
            assert!(!is_settled(lifecycle), "{lifecycle} bukan terminal");
        }
    }

    #[test]
    fn progress_and_suspension_never_close_the_stream() {
        // `clarification.required` menangguhkan job, bukan mengakhirinya: klien
        // boleh menutup koneksi sendiri, tetapi server tidak boleh menyatakan
        // job selesai.
        for event in [
            "job.accepted",
            "job.phase_changed",
            "node.status_changed",
            "clarification.required",
            "clarification.accepted",
            "clarification.auto_resolved",
            "job.resumed",
            "job.notice",
        ] {
            assert!(!is_terminal(event), "{event} tidak boleh menutup stream");
        }
    }
}
