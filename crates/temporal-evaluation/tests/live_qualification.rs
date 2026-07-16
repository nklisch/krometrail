use temporal_evaluation::{
    BrowserAvailability, BrowserProduct, CacheDisposition, CaptureQualificationMeasurements,
    CleanupQualificationMeasurements, ControlQualificationMeasurements, DURATIONS_MS,
    DurationQualificationMeasurement, EvaluationStatus, FailureRecord, LIVE_NON_CLAIMS,
    LIVE_QUALIFICATION_PROFILE, LatencyQualificationMeasurements, LiveQualification,
    QualificationEvidenceMode, QualificationGateId, QualificationGateResult,
    RecoveryQualificationMeasurements, ResourceQualificationMeasurements,
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
        evidence_mode: QualificationEvidenceMode::OperatorAuthorizedLiveCapture,
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
                    status: EvaluationStatus::Pass,
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
            sample_count: 1,
            rss_bytes: vec![1],
            cpu_millis: vec![1],
            browser_child_accounting_available: true,
            unavailable_reason: None,
        },
        latency: LatencyQualificationMeasurements {
            source_interval_id: "interval-1".into(),
            viewport: Viewport {
                width: VIEWPORT_WIDTH,
                height: VIEWPORT_HEIGHT,
            },
            frame_width: 1_920,
            frame_height: 1_080,
            warm_cache: CacheDisposition::Warm,
            temporal_query_elapsed_ms: vec![1, 1],
            artifact_elapsed_ms: vec![1, 1],
            sample_count: 4,
            threshold_profile_ids: vec![
                "evaluation-cached-temporal-bundle-below-1s".into(),
                "evaluation-uncached-storyboard-difference-map-below-5s".into(),
            ],
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
    run.non_claims = LIVE_NON_CLAIMS
        .iter()
        .map(|claim| (*claim).into())
        .collect();
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
    assert_eq!(manifest.krometrail.capture_config.every_nth_frame, 1);
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
fn wrong_capture_viewport_is_recordable_only_as_a_blocked_profile() {
    let mut manifest = live_manifest();
    let qualification = manifest.qualification.as_mut().unwrap();
    qualification.capture.observed_viewport.width = 801;
    qualification.gates[0].status = EvaluationStatus::Blocked;
    qualification.gates[0].failure = Some(failure(RunFailureCode::Unavailable));
    manifest.status = EvaluationStatus::Blocked;
    manifest.failure = Some(failure(RunFailureCode::Unavailable));
    for row in &mut manifest.rows {
        row.status = EvaluationStatus::Blocked;
        row.failure = Some(failure(RunFailureCode::Unavailable));
    }
    manifest.validate().unwrap();

    let mut unsafe_pass = manifest;
    unsafe_pass.qualification.as_mut().unwrap().gates[0].status = EvaluationStatus::Pass;
    unsafe_pass.qualification.as_mut().unwrap().gates[0].failure = None;
    assert!(unsafe_pass.validate().is_err());
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
fn live_non_claims_are_fixed_and_configuration_scoped() {
    let manifest = live_manifest();
    assert_eq!(
        manifest.qualification.as_ref().unwrap().evidence_mode,
        QualificationEvidenceMode::OperatorAuthorizedLiveCapture
    );
    assert_eq!(
        manifest.non_claims,
        LIVE_NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_owned())
            .collect::<Vec<_>>()
    );
    assert!(
        manifest
            .non_claims
            .iter()
            .any(|claim| claim.contains("declared configuration"))
    );
    assert!(
        manifest
            .non_claims
            .iter()
            .any(|claim| claim.contains("macOS") && claim.contains("high-DPI"))
    );
}

#[test]
fn passing_live_manifest_rejects_missing_measurements_and_cleanup() {
    let mut missing_resource = live_manifest();
    let qualification = missing_resource.qualification.as_mut().unwrap();
    qualification.resources.sample_count = 0;
    qualification.resources.rss_bytes.clear();
    qualification.resources.cpu_millis.clear();
    qualification.resources.unavailable_reason =
        Some("platform resource measurement unavailable".into());
    assert!(missing_resource.validate().is_err());

    let mut unresolved_gap = live_manifest();
    let qualification = unresolved_gap.qualification.as_mut().unwrap();
    qualification.capture.gap_ids = vec!["gap-1".into()];
    qualification.capture.gap_count = 1;
    unresolved_gap.artifact.gap_ids = vec!["gap-1".into()];
    assert!(unresolved_gap.validate().is_err());

    let mut failed_control = live_manifest();
    failed_control
        .qualification
        .as_mut()
        .unwrap()
        .control
        .failed_observation_ids = vec!["control:failed".into()];
    assert!(failed_control.validate().is_err());

    let mut cleanup_failure = live_manifest();
    cleanup_failure
        .qualification
        .as_mut()
        .unwrap()
        .cleanup
        .remaining_managed_resources = 1;
    assert!(cleanup_failure.validate().is_err());
}

#[test]
fn live_status_precedence_rejects_a_lower_status_claim() {
    let mut manifest = live_manifest();
    let qualification = manifest.qualification.as_mut().unwrap();
    qualification.gates[0].status = EvaluationStatus::Fail;
    qualification.gates[0].failure = Some(failure(RunFailureCode::Threshold));
    qualification.gates[1].status = EvaluationStatus::Inconclusive;
    qualification.gates[1].failure = Some(failure(RunFailureCode::InsufficientEvidence));
    manifest.status = EvaluationStatus::Fail;
    manifest.failure = Some(failure(RunFailureCode::Threshold));
    assert!(manifest.validate().is_err());
    manifest.status = EvaluationStatus::Inconclusive;
    manifest.failure = Some(failure(RunFailureCode::InsufficientEvidence));
    manifest.validate().unwrap();
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
