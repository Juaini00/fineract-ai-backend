//! Envelope seragam untuk seluruh response HTTP JSON.
//!
//! Kontrak `docs/contracts/api.md` + AGENTS.md: setiap response berbentuk
//! `{ success, data, error }` dengan cabang yang **tidak aktif** bernilai
//! `null` — bukan dihilangkan. Dengan begitu bentuk response deterministik dan
//! klien tidak perlu menebak apakah sebuah key "ada" atau "tidak".

use serde::{Deserialize, Serialize};

/// Pembungkus standar `{ success, data, error }`.
///
/// `data` hanya terisi saat `success == true`; `error` hanya terisi saat
/// `success == false`. Cabang nonaktif selalu diserialisasi sebagai `null`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ErrorBody>,
}

/// Body error publik. **Wajib sudah tersanitasi** — SQL, prompt, dan stack
/// tidak pernah boleh sampai ke sini (AGENTS.md). Detail internal masuk
/// tracing/log, bukan response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorBody {
    /// Kode machine-readable, mis. `CONFLICT`, `INVALID`, `INTERNAL`.
    pub code: String,
    /// Pesan yang aman ditampilkan ke pengguna.
    pub message: String,
}

impl<T> Envelope<T> {
    /// Response sukses: `{ success: true, data, error: null }`.
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Response gagal: `{ success: false, data: null, error }`.
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ErrorBody {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_serializes_with_null_error_branch() {
        let envelope = Envelope::ok(serde_json::json!({ "job_id": "job_01" }));
        let json = serde_json::to_value(&envelope).unwrap();

        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["job_id"], "job_01");
        assert_eq!(json["error"], serde_json::Value::Null);
    }

    #[test]
    fn err_serializes_with_null_data_branch() {
        let envelope = Envelope::<()>::err("CONFLICT", "another job is running");
        let json = serde_json::to_value(&envelope).unwrap();

        assert_eq!(json["success"], false);
        assert_eq!(json["data"], serde_json::Value::Null);
        assert_eq!(json["error"]["code"], "CONFLICT");
        assert_eq!(json["error"]["message"], "another job is running");
    }

    #[test]
    fn envelope_round_trips() {
        let envelope = Envelope::err("INVALID", "answer out of scope");
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: Envelope<serde_json::Value> = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, envelope);
    }
}
