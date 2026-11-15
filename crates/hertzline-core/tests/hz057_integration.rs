//! HZ-057 integration smoke on the host OS (AC-1--AC-8).
//!
//! Live probes and collectors run on the real machine/CI runner. Mutating
//! paths use labeled mocks except Windows power-plan apply/restore/verify.
//! Crash-rollback and benchmark reproducibility are required checks.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[cfg(windows)]
use hertzline_core::action::restore_with_log;
use hertzline_core::action::{
    apply_with_snapshot, validate_explanation, Action, LocalMeasurementNote, MockToggleAction,
};
use hertzline_core::actions::windows::{
    MockPowerPlanBackend, PowerPlanAction, GUID_BALANCED, GUID_HIGH_PERFORMANCE,
};
use hertzline_core::actions::{macos_capability_matrix, MACOS_HONESTY_BANNER};
use hertzline_core::benchmark::{
    evaluate_benchmarks, run_benchmark, BenchmarkConfig, ChangeVerdict, FrameWindowSource,
};
use hertzline_core::collectors::{
    DiskCollector, HardwareCollector, HardwareSample, SysinfoDiskIo, SysinfoHardwareSampler,
};
use hertzline_core::everyday::{
    assess_thermal, generate_weekly_report, list_resource_hogs, refuse_kill_without_confirm,
    MeasurementArchive, MeasurementSummary, SysinfoResourceHogs,
};
use hertzline_core::frame_capture::{probe_frame_capture, ProbeStatus};
use hertzline_core::games::{detect_games, DetectionRoots, GameSource};
use hertzline_core::overlay::{decide_for_install_path, probe_overlay_method, OverlaySettings};
use hertzline_core::profiles::{
    recover_active_profile_after_crash, write_active_marker, ActiveProfileMarker, ProfilePhase,
};
use hertzline_core::restore::{recover_from_crash, restore_all, MapActionRegistry, RestoreTrigger};
use hertzline_core::sensors::probe_sensors;
use hertzline_core::session::{
    export_csv, export_json, parse_csv, parse_json, MeasurementSession, SessionMeta,
};
use hertzline_core::snapshot::{unix_ms_now, SnapshotStore};

/// Labeled mock frame windows (not live capture).
struct LabeledMockWindows {
    windows: Vec<Vec<f64>>,
    next: usize,
}

impl LabeledMockWindows {
    fn new(windows: Vec<Vec<f64>>) -> Self {
        Self { windows, next: 0 }
    }
}

impl FrameWindowSource for LabeledMockWindows {
    fn collect_window(
        &mut self,
        _duration_ms: u64,
    ) -> Result<Vec<f64>, hertzline_core::benchmark::BenchmarkError> {
        if self.next >= self.windows.len() {
            return Err(hertzline_core::benchmark::BenchmarkError::Capture(
                "labeled mock: no more windows".into(),
            ));
        }
        let w = self.windows[self.next].clone();
        self.next += 1;
        Ok(w)
    }
}

fn temp_dir(label: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "hertzline-hz057-{label}-{}-{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("games")
        .join("fixtures")
}

fn constant_frames(ms: f64, n: usize) -> Vec<f64> {
    vec![ms; n]
}

#[test]
fn hz057_host_banner() {
    println!(
        "hz057_host os={} arch={} crate={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        hertzline_core::CRATE_VERSION
    );
}

#[test]
fn ac1_measure_smoke_live_probes_and_export() {
    let frame = probe_frame_capture();
    assert!(!frame.injects_into_game, "frame probe must never inject");
    assert!(matches!(
        frame.status,
        ProbeStatus::Available | ProbeStatus::Unavailable | ProbeStatus::NotPossible
    ));

    let sensors = probe_sensors();
    assert_eq!(sensors.primary_license_spdx, "MIT");
    assert!(!sensors.metrics.is_empty());

    let mut hw = HardwareCollector::new(SysinfoHardwareSampler::new());
    let mut disk = DiskCollector::new(SysinfoDiskIo).with_interval(Duration::from_millis(50));

    let mut session = MeasurementSession::new(SessionMeta {
        session_id: "hz057-ac1".into(),
        started_at_unix_ms: unix_ms_now(),
        ended_at_unix_ms: None,
        target_process: Some("hz057".into()),
        os: std::env::consts::OS.to_string(),
        notes: Some("integration smoke".into()),
    });
    session.hardware = hw.collect();
    session.disk = disk.collect();
    session.complete(unix_ms_now());

    let csv = export_csv(&session);
    let json = export_json(&session).expect("json");
    let csv_rows = parse_csv(&csv).expect("parse csv");
    let json_doc = parse_json(&json).expect("parse json");
    assert_eq!(csv_rows.len(), json_doc.fields.len());
    for (a, b) in csv_rows.iter().zip(json_doc.fields.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.unit, b.unit);
    }
}

#[test]
fn ac2_benchmark_reproducibility_and_noise_verdict() {
    let windows = vec![
        constant_frames(16.0, 8),
        constant_frames(16.0, 40),
        constant_frames(16.5, 40),
        constant_frames(15.5, 40),
    ];
    let cfg = BenchmarkConfig {
        warmup_ms: 100,
        measure_ms: 500,
        pass_count: 3,
    };
    let a =
        run_benchmark(cfg.clone(), &mut LabeledMockWindows::new(windows.clone())).expect("run a");
    let b = run_benchmark(cfg, &mut LabeledMockWindows::new(windows)).expect("run b");
    assert_eq!(a.avg_fps, b.avg_fps);
    assert_eq!(a.low_1pct_fps, b.low_1pct_fps);
    assert_eq!(a.passes.len(), 3);

    let verdict = evaluate_benchmarks(&a, &b);
    assert_eq!(verdict.verdict, ChangeVerdict::NoMeasurableEffect);
    assert!(verdict.offer_revert);
}

#[test]
fn ac3_game_fixtures_and_crash_profile_recovery() {
    let base = fixture_dir();
    let roots = DetectionRoots {
        steam_roots: vec![base.join("steam").join("root")],
        epic_manifests: vec![base.join("epic").join("Manifests")],
        gog_libraries: vec![base.join("gog").join("library")],
        xbox_games_roots: vec![base.join("xbox").join("XboxGames")],
        gaming_root_files: vec![],
        battlenet_product_db: vec![base.join("battlenet").join("product.db")],
        lutris_games_dirs: vec![base.join("lutris").join("games")],
        heroic_dirs: vec![base.join("heroic").join("heroic_config")],
        legendary_dirs: vec![base.join("heroic").join("legendary")],
        manual: vec![(
            "Manual Fixture".into(),
            base.join("manual").join("CustomGame"),
        )],
    };
    let games = detect_games(&roots);
    let sources: Vec<_> = games.iter().map(|g| g.source).collect();
    assert!(sources.contains(&GameSource::Steam));
    assert!(sources.contains(&GameSource::Epic));

    let _live = detect_games(&DetectionRoots::from_os_defaults());

    let dir = temp_dir("ac3");
    let store_path = dir.join("snapshots.json");
    let marker_path = dir.join("active_profile.json");
    let mut store = SnapshotStore::open(&store_path).expect("store");
    let mut action = MockToggleAction::new("mock_profile", "off", "on");
    let entry = apply_with_snapshot(&mut action, &mut store, unix_ms_now()).expect("apply");

    let marker = ActiveProfileMarker {
        schema_version: 1,
        game_key: "manual:hz057".into(),
        name: "Fixture".into(),
        action_ids: vec!["mock_profile".into()],
        entry_ids: vec![entry.id],
        started_at_unix_ms: unix_ms_now(),
        phase: ProfilePhase::Active,
    };
    write_active_marker(&marker_path, &marker).expect("marker");
    drop(store);

    let mut registry = MapActionRegistry::new();
    registry.insert(Box::new(MockToggleAction::new("mock_profile", "on", "on")));
    let report = recover_active_profile_after_crash(&marker_path, &store_path, &mut registry)
        .expect("recover")
        .expect("had active");
    assert!(report.ok);
    assert_eq!(report.trigger, RestoreTrigger::Crash);
    assert!(!marker_path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ac4_action_explanations_and_host_capability() {
    let backend = RefCell::new(MockPowerPlanBackend::new(
        GUID_BALANCED,
        &[GUID_BALANCED, GUID_HIGH_PERFORMANCE],
    ));
    let action = PowerPlanAction::high_performance(backend);
    validate_explanation(action.explanation()).expect("ac-8.1");
    assert_eq!(
        action.explanation().local_measurement.message(),
        LocalMeasurementNote::NONE_YET_MESSAGE
    );

    #[cfg(windows)]
    {
        use hertzline_core::actions::windows::power_plan::os::OsPowerPlanBackend;
        let mut live = PowerPlanAction::high_performance(OsPowerPlanBackend);
        let before = live.read_state().expect("live power plan read");
        assert!(!before.is_empty());
        let dir = temp_dir("ac4-win");
        let store_path = dir.join("snapshots.json");
        let mut store = SnapshotStore::open(&store_path).expect("store");
        match apply_with_snapshot(&mut live, &mut store, unix_ms_now()) {
            Ok(e) => {
                restore_with_log(&mut live, &mut store, e.id).expect("restore power plan");
                assert_eq!(live.read_state().expect("after restore"), before);
            }
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("not present")
                        || msg.contains("not possible")
                        || msg.contains("failed")
                        || msg.contains("scheme"),
                    "unexpected apply err: {msg}"
                );
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }

    assert!(!macos_capability_matrix().is_empty());
    assert!(MACOS_HONESTY_BANNER.contains("macOS"));
}

#[test]
fn ac5_everyday_smoke_no_silent_kill() {
    assert!(refuse_kill_without_confirm(false).is_err());
    assert!(refuse_kill_without_confirm(true).is_err());

    let mut backend = SysinfoResourceHogs::default();
    let report = list_resource_hogs(&mut backend, 5).expect("hogs");
    assert!(report.note.contains("never kills") || report.note.contains("Report only"));

    let sample = HardwareSample {
        cpu_usage_percent: Some(10.0),
        ram_used_bytes: Some(1.0),
        ram_total_bytes: Some(2.0),
        gpu_usage_percent: None,
        vram_used_bytes: None,
        vram_total_bytes: None,
        cpu_temp_celsius: None,
        gpu_temp_celsius: None,
        cpu_clock_mhz: None,
        gpu_clock_mhz: None,
        throttling: None,
    };
    let thermal = assess_thermal(&sample);
    assert!(!thermal.note.is_empty());

    let dir = temp_dir("ac5");
    let store = SnapshotStore::open(dir.join("snapshots.json")).expect("store");
    let archive = MeasurementArchive::new();
    let weekly = generate_weekly_report(&store, &archive, unix_ms_now());
    assert!(weekly.window_end_unix_ms >= weekly.window_start_unix_ms);
    let _ = MeasurementSummary {
        session_id: "hz057".into(),
        started_at_unix_ms: unix_ms_now(),
        ended_at_unix_ms: None,
        target_process: None,
        os: std::env::consts::OS.to_string(),
        frame_status: "unavailable".into(),
        notes: None,
    };
    let _ = hertzline_core::everyday::list_startup_impact();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ac6_overlay_anti_cheat_default_off_and_global_disable() {
    let method = probe_overlay_method();
    assert!(!method.injects_into_game);

    let dir = temp_dir("ac6");
    let game = dir.join("game");
    fs::create_dir_all(game.join("EasyAntiCheat")).expect("eac");
    let mut settings = OverlaySettings {
        globally_disabled: false,
        user_enabled: true,
        allow_with_anti_cheat: false,
    };
    let decision = decide_for_install_path(&settings, Some(&game));
    assert!(
        !decision.enabled,
        "anti-cheat must keep overlay off by default"
    );

    settings.disable_globally();
    let blocked = decide_for_install_path(&settings, None);
    assert!(!blocked.enabled);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ac7_crash_rollback_and_restore_all() {
    let dir = temp_dir("ac7");
    let store_path = dir.join("snapshots.json");
    {
        let mut store = SnapshotStore::open(&store_path).expect("store");
        let mut action = MockToggleAction::new("mock_hz057", "off", "on");
        apply_with_snapshot(&mut action, &mut store, unix_ms_now()).expect("apply");
        assert_eq!(action.state(), "on");
    }

    let mut registry = MapActionRegistry::new();
    registry.insert(Box::new(MockToggleAction::new("mock_hz057", "on", "on")));
    let crash = recover_from_crash(&store_path, &mut registry).expect("crash");
    assert!(crash.ok);
    assert_eq!(crash.trigger, RestoreTrigger::Crash);
    assert!(!crash.requires_ui);
    assert_eq!(
        registry.get("mock_hz057").unwrap().read_state().unwrap(),
        "off"
    );

    let mut empty = MapActionRegistry::new();
    let mut store2 = SnapshotStore::load_from_path(&store_path).expect("reload");
    let explicit = restore_all(&mut store2, &mut empty, RestoreTrigger::Explicit);
    assert!(!explicit.requires_ui);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ac8_explanation_local_measurement_note() {
    let action = MockToggleAction::new("mock_explain", "a", "b");
    validate_explanation(action.explanation()).expect("fields");
    assert_eq!(
        action.explanation().local_measurement.message(),
        "no local measurement exists yet"
    );
    let effect = action.explanation().expected_effect.to_lowercase();
    assert!(
        !effect.contains("+") || !effect.contains("fps"),
        "must not claim fabricated +X FPS: {effect}"
    );
}
