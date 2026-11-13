//! IPC stubs shared by the desktop shell and (later) the privileged helper client.
//!
//! Named operations only. No arbitrary shell.

use serde::{Deserialize, Serialize};

/// Allow-listed helper operation names for the D1 IPC shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelperOp {
    Ping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingRequest {
    pub client: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingResponse {
    pub ok: bool,
    pub message: String,
    pub core_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperRequest {
    pub op: HelperOp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperResponse {
    pub accepted: bool,
    pub detail: String,
}

/// UI <-> core ping used by the D1 Tauri command bridge.
pub fn ping(request: &PingRequest) -> PingResponse {
    let _ = request;
    PingResponse {
        ok: true,
        message: "pong".into(),
        core_version: crate::CRATE_VERSION.into(),
    }
}

/// Simulated helper broker: accept only allow-listed ops.
pub fn helper_dispatch(request: &HelperRequest) -> HelperResponse {
    match request.op {
        HelperOp::Ping => HelperResponse {
            accepted: true,
            detail: "helper_pong".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_round_trip_json() {
        let req = PingRequest {
            client: "d1-tauri".into(),
        };
        let res = ping(&req);
        assert!(res.ok);
        assert_eq!(res.message, "pong");
        let encoded = serde_json::to_string(&res).expect("serialize");
        let decoded: PingResponse = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, res);
    }

    #[test]
    fn helper_allow_list_ping() {
        let res = helper_dispatch(&HelperRequest { op: HelperOp::Ping });
        assert!(res.accepted);
        assert_eq!(res.detail, "helper_pong");
    }
}
