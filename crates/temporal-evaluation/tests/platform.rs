use temporal_evaluation::{
    BrowserAvailability, BrowserProduct, CacheDisposition, CaptureQualificationMeasurements,
    CleanupQualificationMeasurements, ControlQualificationMeasurements, DURATIONS_MS,
    DurationQualificationMeasurement, EvaluationStatus, FailureRecord, LiveQualification,
    PLATFORM_EVIDENCE_PROFILE, PLATFORM_NON_CLAIMS, PlatformLaneId, QualificationEvidenceMode,
    QualificationGateId, QualificationGateResult, RecoveryQualificationMeasurements,
    ResourceQualificationMeasurements, RetentionQualificationMeasurements, RunFailureCode,
    RunManifest, VIEWPORT_HEIGHT, VIEWPORT_WIDTH, Viewport, validate_platform_lane,
};

fn platform_manifest(lane: PlatformLaneId) -> RunManifest {
    let definition = lane.definition();
    let mut manifest = RunManifest::sample();
    manifest.run.threshold_profile = PLATFORM_EVIDENCE_PROFILE.into();
    manifest.run.repetitions = 30;
    manifest.run.optional_configuration = !definition.required;
    manifest.run.device_scale_factor = definition.requested_device_scale_factor;
    manifest.environment.platform = definition.platform;
    manifest.browser = BrowserAvailability::Observed {
        product: definition.browser_product,
        product_version: "123.0".into(),
        protocol_version: "1.3".into(),
        revision: "123456".into(),
        capability_id: "browser-get-version".into(),
    };
    manifest.platform = Some(definition.declaration());
    manifest.prompt = temporal_evaluation::PromptSet::canonical()
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
    manifest.qualification = Some(LiveQualification {
        profile: PLATFORM_EVIDENCE_PROFILE.into(),
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
            observed_device_scale_factor: definition.minimum_observed_device_scale_factor,
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
        latency: temporal_evaluation::LatencyQualificationMeasurements {
            source_interval_id: "interval-1".into(),
            viewport: Viewport {
                width: VIEWPORT_WIDTH,
                height: VIEWPORT_HEIGHT,
            },
            frame_width: 1_920,
            frame_height: 1_080,
            warm_cache: CacheDisposition::Warm,
            temporal_query_elapsed_ms: vec![1],
            artifact_elapsed_ms: vec![1],
            sample_count: 1,
            threshold_profile_ids: vec!["latency".into()],
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
    manifest.non_claims = PLATFORM_NON_CLAIMS
        .iter()
        .map(|claim| (*claim).to_owned())
        .collect();
    manifest
}

fn failure(code: RunFailureCode) -> FailureRecord {
    FailureRecord {
        code,
        phase: "platform".into(),
        reason: "platform evidence is not decisive".into(),
        recovery: "collect the declared platform lane again".into(),
        retryable: true,
    }
}

#[test]
fn registry_is_exactly_ordered_and_has_no_duplicate_lane_identity() {
    let all = PlatformLaneId::ALL;
    let mut unique = all.to_vec();
    unique.sort_by_key(|lane| lane.as_str());
    unique.dedup();
    assert_eq!(unique.len(), all.len());
    assert_eq!(all.len(), 4);
    assert_eq!(PlatformLaneId::REQUIRED.len(), 3);
    assert!(!PlatformLaneId::REQUIRED.contains(&PlatformLaneId::LinuxChromiumOptional));
}

#[test]
fn platform_manifest_is_canonical_and_round_trips_without_privacy_leaks() {
    let manifest = platform_manifest(PlatformLaneId::MacosChromeHighDpi);
    validate_platform_lane(PlatformLaneId::MacosChromeHighDpi, &manifest).unwrap();
    let bytes = manifest.canonical_bytes().unwrap();
    assert_eq!(RunManifest::from_canonical_json(&bytes).unwrap(), manifest);
    assert_eq!(bytes, manifest.canonical_bytes().unwrap());
    let text = String::from_utf8(bytes).unwrap();
    for forbidden in ["http://", "https://", "/home/", "websocket", "password"] {
        assert!(!text.contains(forbidden));
    }
}

#[test]
fn validation_rejects_wrong_lane_declaration_product_platform_viewport_and_profile() {
    let mut wrong_lane = platform_manifest(PlatformLaneId::MacosChromeDefaultDpi);
    assert!(validate_platform_lane(PlatformLaneId::MacosChromeHighDpi, &wrong_lane).is_err());

    wrong_lane = platform_manifest(PlatformLaneId::MacosChromeDefaultDpi);
    wrong_lane.browser = BrowserAvailability::Observed {
        product: BrowserProduct::Chromium,
        product_version: "123.0".into(),
        protocol_version: "1.3".into(),
        revision: "123456".into(),
        capability_id: "browser-get-version".into(),
    };
    assert!(validate_platform_lane(PlatformLaneId::MacosChromeDefaultDpi, &wrong_lane).is_err());

    wrong_lane = platform_manifest(PlatformLaneId::MacosChromeDefaultDpi);
    wrong_lane.environment.platform = temporal_evaluation::Platform::Linux;
    assert!(validate_platform_lane(PlatformLaneId::MacosChromeDefaultDpi, &wrong_lane).is_err());

    wrong_lane = platform_manifest(PlatformLaneId::MacosChromeDefaultDpi);
    wrong_lane.run.viewport.width += 1;
    assert!(validate_platform_lane(PlatformLaneId::MacosChromeDefaultDpi, &wrong_lane).is_err());

    wrong_lane = platform_manifest(PlatformLaneId::MacosChromeDefaultDpi);
    wrong_lane.run.threshold_profile = temporal_evaluation::LIVE_QUALIFICATION_PROFILE.into();
    assert!(validate_platform_lane(PlatformLaneId::MacosChromeDefaultDpi, &wrong_lane).is_err());
}

#[test]
fn high_dpi_requires_observed_scale_even_when_requested_scale_is_two() {
    let mut manifest = platform_manifest(PlatformLaneId::MacosChromeHighDpi);
    assert_eq!(manifest.run.device_scale_factor, 2_000);
    manifest
        .qualification
        .as_mut()
        .unwrap()
        .capture
        .observed_device_scale_factor = 1_000;
    assert!(validate_platform_lane(PlatformLaneId::MacosChromeHighDpi, &manifest).is_err());
}

#[test]
fn nonpassing_platform_manifests_keep_explicit_failure_and_do_not_become_passes() {
    let mut manifest = platform_manifest(PlatformLaneId::MacosChromeHighDpi);
    manifest.status = EvaluationStatus::Blocked;
    manifest.failure = Some(failure(RunFailureCode::Unavailable));
    for row in &mut manifest.rows {
        row.status = EvaluationStatus::Blocked;
        row.failure = Some(failure(RunFailureCode::Unavailable));
    }
    for gate in &mut manifest.qualification.as_mut().unwrap().gates {
        gate.status = EvaluationStatus::Blocked;
        gate.failure = Some(failure(RunFailureCode::Unavailable));
    }
    manifest
        .qualification
        .as_mut()
        .unwrap()
        .capture
        .observed_device_scale_factor = 0;
    manifest
        .qualification
        .as_mut()
        .unwrap()
        .capture
        .observed_viewport = Viewport {
        width: 0,
        height: 0,
    };
    validate_platform_lane(PlatformLaneId::MacosChromeHighDpi, &manifest).unwrap();
    assert_ne!(manifest.status, EvaluationStatus::Pass);
}

#[test]
fn unknown_lane_values_are_rejected_by_the_typed_contract() {
    let mut value = serde_json::to_value(platform_manifest(
        PlatformLaneId::LinuxStableChromeReferenceHost,
    ))
    .unwrap();
    value["platform"]["lane"] = serde_json::json!("not_registered");
    assert!(RunManifest::from_canonical_json(&serde_json::to_vec(&value).unwrap()).is_err());
}
