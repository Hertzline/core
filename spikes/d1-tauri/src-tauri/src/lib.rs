// MSVC emits .lib/.exp notes when linking cdylib; not actionable for this spike.
#![allow(linker_messages)]

use hertzline_core::ipc::{
    helper_dispatch, ping, HelperOp, HelperRequest, PingRequest, PingResponse,
};

#[tauri::command]
fn core_ping(client: String) -> PingResponse {
    ping(&PingRequest { client })
}

#[tauri::command]
fn helper_ping() -> hertzline_core::ipc::HelperResponse {
    helper_dispatch(&HelperRequest {
        op: HelperOp::Ping,
    })
}

fn run_smoke_and_exit() -> ! {
    let core = core_ping("d1-smoke".into());
    let helper = helper_ping();
    if !core.ok || !helper.accepted {
        eprintln!("d1 smoke failed: core={core:?} helper={helper:?}");
        std::process::exit(1);
    }
    println!(
        "d1_smoke_ok core_version={} helper={}",
        core.core_version, helper.detail
    );
    std::process::exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Run IPC smoke before any window/WebView init so headless CI works.
    if std::env::var_os("HERTZLINE_D1_SMOKE").is_some() {
        run_smoke_and_exit();
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![core_ping, helper_ping])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
