use temporal_evaluation::{
    BrowserAvailability, BrowserProduct, CacheDisposition, CaptureQualificationMeasurements,
    CleanupQualificationMeasurements, ControlQualificationMeasurements, DURATIONS_MS,
    DurationQualificationMeasurement, EvaluationStatus, FailureRecord, LIVE_QUALIFICATION_PROFILE,
    LatencyQualificationMeasurements, LiveQualification, QualificationGateId,
    QualificationGateResult, RecoveryQualificationMeasurements, ResourceQualificationMeasurements,
    RetentionQualificationMeasurements, RunFailureCode, RunManifest, VIEWPORT_HEIGHT,
    VIEWPORT_WIDTH, Viewport,
};

fn live_manifest() -> RunManifest {
    let mut run = RunManifest::sample();
    run.run.threshold_profile = LIVE_QUALIFICATION_PROFILE.into();
    run.run.repetitions = 30;
    run.browser = BrowserAvailability::Observed {
        product: BrowserProduct::Chrome,
        product_version: "123.0".into(),
        protocol_version: "1.3".into(),
        revision: "123456".into(),
        capability_id: "browser-get-version".into(),
    };
    run.prompt = temporal_evaluation::PromptSet::canonical()
        .template(temporal_evaluation::PromptId::CaptureQualification)
        .cloned()
        .map(|template| temporal_evaluation::ManifestPrompt {
            prompt_set_id: template.id,
            prompt_version: template.version,
            system_prompt: template.system_prompt,
            task_prompt: template.task_prompt,
            sha256: template.sha256,
        })
        .unwrap();
    run.qualification = Some(LiveQualification {
        profile: LIVE_QUALIFICATION_PROFILE.into(),
        gates: QualificationGateId::ALL
            .into_iter()
            .map(|gate| QualificationGateResult {
                gate,
                status: EvaluationStatus::Pass,
                failure: None,
            })
            .collect(),
        capture: CaptureQualificationMeasurements {
            requested_durations_ms: DURATIONS_MS.to_vec(),
            repetitions: 30,
            observed_viewport: Viewport {
                width: VIEWPORT_WIDTH,
                height: VIEWPORT_HEIGHT,
            },
            observed_device_scale_factor: 1_000,
            source_frame_count: 1,
            observed_frame_count: 1,
            source_time_sample_count: 1,
            gap_ids: Vec::new(),
            gap_count: 0,
            per_duration: DURATIONS_MS
                .iter()
                .map(|duration_ms| DurationQualificationMeasurement {
                    duration_ms: *duration_ms,
                    eligible_count: 1,
                    observed_count: 1,
                    eligibility_rate_basis_points: 10_000,
                    coverage_rate_basis_points: 10_000,
                })
                .collect(),
        },
        control: ControlQualificationMeasurements {
            scenario_ids: vec!["navigation".into()],
            attempts: 1,
            successes: 1,
            failed_observation_ids: Vec::new(),
            success_rate_basis_points: 10_000,
        },
        retention: RetentionQualificationMeasurements {
            budget_bytes: 1,
            peak_usage_bytes: 1,
            pinned_interval_preserved: true,
            evicted_frame_count: 0,
            capture_paused_when_pinned: false,
            capture_resumed_after_unpin: true,
            cleanup_removed_frame_count: 0,
        },
        recovery: RecoveryQualificationMeasurements {
            reopened: true,
            reconciled: true,
            recovered_frame_count: 1,
            removed_frame_count: 0,
            trailing_segment_repaired: false,
            staged_artifacts_recovered: true,
        },
        resources: ResourceQualificationMeasurements {
            sample_count: 0,
            rss_bytes: Vec::new(),
            cpu_millis: Vec::new(),
            browser_child_accounting_available: false,
            unavailable_reason: Some("platform resource adapter unavailable".into()),
        },
        latency: LatencyQualificationMeasurements {
            source_interval_id: "interval-1".into(),
            viewport: Viewport {
                width: VIEWPORT_WIDTH,
                height: VIEWPORT_HEIGHT,
            },
            frame_width: VIEWPORT_WIDTH,
            frame_height: VIEWPORT_HEIGHT,
            warm_cache: CacheDisposition::Unavailable,
            temporal_query_elapsed_ms: Vec::new(),
            artifact_elapsed_ms: Vec::new(),
            sample_count: 0,
            threshold_profile_ids: vec!["not-applicable".into()],
        },
        cleanup: CleanupQualificationMeasurements {
            server_stopped: true,
            profile_deleted: true,
            store_flushed: true,
            lock_released: true,
            output_finalized: false,
            remaining_managed_resources: 0,
        },
    });
    run
}

fn failure(code: RunFailureCode) -> FailureRecord {
    FailureRecord {
        code,
        phase: "qualification".into(),
        reason: "qualification evidence is incomplete".into(),
        recovery: "collect the missing qualification evidence".into(),
        retryable: true,
    }
}

#[test]
fn live_profile_round_trips_and_excludes_outcomes_from_input_digest() {
    let manifest = live_manifest();
    manifest.validate().unwrap();
    let digest = manifest.input_digest().unwrap();
    let bytes = manifest.canonical_bytes().unwrap();
    let round_trip = RunManifest::from_canonical_json(&bytes).unwrap();
    assert_eq!(round_trip, manifest);

    let mut changed = manifest;
    changed
        .qualification
        .as_mut()
        .unwrap()
        .capture
        .source_frame_count += 1;
    changed.qualification.as_mut().unwrap().gates[0].status = EvaluationStatus::Fail;
    changed.qualification.as_mut().unwrap().gates[0].failure =
        Some(failure(RunFailureCode::Threshold));
    changed.status = EvaluationStatus::Fail;
    changed.failure = Some(failure(RunFailureCode::Threshold));
    assert_eq!(changed.input_digest().unwrap(), digest);
}

#[test]
fn live_registry_rejects_duplicate_or_unknown_gate_sets() {
    let mut duplicate = live_manifest();
    duplicate.qualification.as_mut().unwrap().gates[1].gate = QualificationGateId::ALL[0];
    assert!(duplicate.validate().is_err());

    let mut unknown = serde_json::to_value(live_manifest()).unwrap();
    unknown["qualification"]["gates"][0]["gate"] = serde_json::json!("not-a-gate");
    assert!(RunManifest::from_canonical_json(&serde_json::to_vec(&unknown).unwrap()).is_err());
}

#[test]
fn live_failed_gate_can_support_fail_without_a_failed_trial_row() {
    let mut manifest = live_manifest();
    let qualification = manifest.qualification.as_mut().unwrap();
    qualification.gates[1].status = EvaluationStatus::Fail;
    qualification.gates[1].failure = Some(failure(RunFailureCode::Threshold));
    manifest.status = EvaluationStatus::Fail;
    manifest.failure = Some(failure(RunFailureCode::Threshold));
    manifest.validate().unwrap();
}

#[test]
fn live_privacy_rejects_unsafe_measurement_text() {
    let mut manifest = live_manifest();
    manifest
        .qualification
        .as_mut()
        .unwrap()
        .resources
        .unavailable_reason = Some("https://127.0.0.1/private".into());
    assert!(manifest.validate().is_err());
}

#[test]
fn live_blocked_and_optional_skipped_statuses_remain_explicit() {
    let mut blocked = live_manifest();
    blocked.browser = BrowserAvailability::Blocked {
        reason: "required browser installation is unavailable".into(),
        recovery: "install a supported local browser and retry".into(),
    };
    blocked.status = EvaluationStatus::Blocked;
    blocked.failure = Some(failure(RunFailureCode::Unavailable));
    for row in &mut blocked.rows {
        row.status = EvaluationStatus::Blocked;
        row.failure = Some(failure(RunFailureCode::Unavailable));
    }
    for gate in &mut blocked.qualification.as_mut().unwrap().gates {
        gate.status = EvaluationStatus::Blocked;
        gate.failure = Some(failure(RunFailureCode::Unavailable));
    }
    blocked.validate().unwrap();

    let mut skipped = live_manifest();
    skipped.run.optional_configuration = true;
    skipped.browser = BrowserAvailability::Skipped {
        product: BrowserProduct::Chromium,
        reason: "optional Linux Chromium is unavailable".into(),
        recovery: "install the optional Linux Chromium configuration before collecting it".into(),
    };
    skipped.status = EvaluationStatus::Skipped;
    skipped.failure = Some(failure(RunFailureCode::OptionalUnavailable));
    for row in &mut skipped.rows {
        row.status = EvaluationStatus::Skipped;
        row.failure = Some(failure(RunFailureCode::OptionalUnavailable));
    }
    for gate in &mut skipped.qualification.as_mut().unwrap().gates {
        gate.status = EvaluationStatus::Skipped;
        gate.failure = Some(failure(RunFailureCode::OptionalUnavailable));
    }
    skipped.validate().unwrap();
}
