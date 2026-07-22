use krometrail_core::{
    BrowserOperationKind, BrowserOperationResult, ErrorCode, ErrorContext, IdSource,
    InteractionAnchor, InteractionEvidenceSink, InteractionOutcome, InteractionPostcondition,
    InteractionRecord, KrometrailError, LocatorSummary, MonotonicClock, NavigationId, NonEmptyText,
    ObservationContext, PageChange, PageOperationOutcome, Result, RetryAdvice, SanitizedParameters,
};

struct EvidenceProjection {
    anchor: InteractionAnchor,
    record: Option<InteractionRecord>,
    navigation_id: Option<NavigationId>,
}

pub(crate) async fn persist_result_evidence(
    result: &BrowserOperationResult,
    sink: &dyn InteractionEvidenceSink,
    clock: &dyn MonotonicClock,
    ids: &dyn IdSource,
) -> Result<()> {
    let Some(projection) = project_result(result, ids)? else {
        return Ok(());
    };
    let context = ErrorContext {
        session_id: Some(projection.anchor.session_id),
        target_id: Some(projection.anchor.target_id),
        interaction_id: Some(projection.anchor.interaction_id),
        range: None,
    };
    sink.append_operation_evidence(
        projection.anchor,
        projection.record,
        clock.now(),
        projection.navigation_id,
    )
    .await
    .map_err(|_| persistence_uncertainty(context))
}

fn project_result(
    result: &BrowserOperationResult,
    ids: &dyn IdSource,
) -> Result<Option<EvidenceProjection>> {
    let page = match result {
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ActivatePage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => Some(value.as_ref()),
        BrowserOperationResult::SetViewport(value) => Some(&value.operation),
        _ => None,
    };
    if let Some(page) = page {
        let navigation_id = matches!(
            (&page.outcome, page.interaction.operation),
            (
                PageOperationOutcome::Succeeded(PageChange::Navigated),
                BrowserOperationKind::NavigatePage
            ) | (
                PageOperationOutcome::Succeeded(PageChange::Reloaded),
                BrowserOperationKind::ReloadPage
            ) | (
                PageOperationOutcome::Succeeded(PageChange::WentBack),
                BrowserOperationKind::GoBack
            ) | (
                PageOperationOutcome::Succeeded(PageChange::WentForward),
                BrowserOperationKind::GoForward
            )
        )
        .then(|| NavigationId::from_uuid(*ids.next().as_uuid()));
        return Ok(Some(EvidenceProjection {
            anchor: page.interaction.clone(),
            record: None,
            navigation_id,
        }));
    }

    if let BrowserOperationResult::WriteClipboard(value) = result {
        let anchor = value.operation.interaction.clone();
        let record = InteractionRecord::new(
            anchor.interaction_id,
            ObservationContext::new(
                anchor.session_id,
                anchor.target_id,
                0,
                anchor.timing.started_at,
                anchor.timing.completed_at,
            )?,
            anchor.timing.dispatched_at,
            anchor.timing.completed_at,
            BrowserOperationKind::WriteClipboard,
            SanitizedParameters::new(serde_json::json!({"utf8_bytes": value.utf8_bytes}))?,
            LocatorSummary::from_locator(None),
            InteractionOutcome::Dispatched,
            InteractionPostcondition::unobserved(),
            None,
        )?;
        return Ok(Some(EvidenceProjection {
            anchor,
            record: Some(record),
            navigation_id: None,
        }));
    }

    if let BrowserOperationResult::CancelDownload(value) = result {
        let anchor = value.operation.clone();
        let record = InteractionRecord::new(
            anchor.interaction_id,
            ObservationContext::new(
                anchor.session_id,
                anchor.target_id,
                0,
                anchor.timing.started_at,
                anchor.timing.completed_at,
            )?,
            anchor.timing.dispatched_at,
            anchor.timing.completed_at,
            BrowserOperationKind::CancelDownload,
            SanitizedParameters::new(serde_json::json!({
                "download_id": value.download_id,
                "state": value.state,
            }))?,
            LocatorSummary::from_locator(None),
            InteractionOutcome::Dispatched,
            InteractionPostcondition::unobserved(),
            None,
        )?;
        return Ok(Some(EvidenceProjection {
            anchor,
            record: Some(record),
            navigation_id: None,
        }));
    }

    let action = match result {
        BrowserOperationResult::Click(value)
        | BrowserOperationResult::Fill(value)
        | BrowserOperationResult::PressKeys(value)
        | BrowserOperationResult::SelectOption(value)
        | BrowserOperationResult::Hover(value)
        | BrowserOperationResult::Drag(value)
        | BrowserOperationResult::Scroll(value)
        | BrowserOperationResult::UploadFiles(value)
        | BrowserOperationResult::HandleDialog(value) => Some(value.as_ref()),
        BrowserOperationResult::InspectPage(_)
        | BrowserOperationResult::SnapshotPage(_)
        | BrowserOperationResult::QueryPage(_)
        | BrowserOperationResult::TakeScreenshot(_)
        | BrowserOperationResult::EvaluatePage(_)
        | BrowserOperationResult::ObserveLive(_)
        | BrowserOperationResult::ListPages(_)
        | BrowserOperationResult::ListPageContexts(_)
        | BrowserOperationResult::WaitForPage(_)
        | BrowserOperationResult::ListFrames(_)
        | BrowserOperationResult::ListPageAssets(_)
        | BrowserOperationResult::ReadClipboard(_)
        | BrowserOperationResult::ListDownloads(_)
        | BrowserOperationResult::WaitForDownload(_)
        | BrowserOperationResult::Wait(_)
        | BrowserOperationResult::Batch(_) => None,
        BrowserOperationResult::CreatePage(_)
        | BrowserOperationResult::SelectPage(_)
        | BrowserOperationResult::ActivatePage(_)
        | BrowserOperationResult::ClosePage(_)
        | BrowserOperationResult::NavigatePage(_)
        | BrowserOperationResult::ReloadPage(_)
        | BrowserOperationResult::GoBack(_)
        | BrowserOperationResult::GoForward(_)
        | BrowserOperationResult::SetViewport(_) => unreachable!("page results handled above"),
        BrowserOperationResult::WriteClipboard(_) => {
            unreachable!("clipboard result handled above")
        }
        BrowserOperationResult::CancelDownload(_) => {
            unreachable!("download cancellation handled above")
        }
    };
    action
        .map(|action| {
            Ok(EvidenceProjection {
                anchor: action.anchor()?,
                record: Some(action.record.clone()),
                navigation_id: None,
            })
        })
        .transpose()
}

fn persistence_uncertainty(context: ErrorContext) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new(
            "browser state changed but its temporal evidence could not be committed",
        )
        .expect("static persistence error is non-empty"),
    )
    .with_context(context)
    .with_retry(RetryAdvice::Never)
    .with_recovery(
        NonEmptyText::new(
            "inspect the current page and recording status before deciding whether to repeat the action",
        )
        .expect("static persistence recovery is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        IdValue, InteractionId, InteractionTiming, ObservationPart, PageOperationResult, SessionId,
        SessionTime, TargetId,
    };
    use uuid::Uuid;

    struct FixedIds;

    impl IdSource for FixedIds {
        fn next(&self) -> IdValue {
            IdValue::from_uuid(Uuid::from_u128(99))
        }
    }

    fn page_result(
        operation: BrowserOperationKind,
        change: PageChange,
    ) -> krometrail_core::PageOperationResult {
        let target = TargetId::from_uuid(Uuid::from_u128(2));
        PageOperationResult::new(
            InteractionAnchor::new(
                InteractionId::from_uuid(Uuid::from_u128(3)),
                SessionId::from_uuid(Uuid::from_u128(1)),
                target,
                operation,
                InteractionTiming::new(
                    SessionTime::from_nanos(1),
                    SessionTime::from_nanos(2),
                    SessionTime::from_nanos(3),
                    None,
                )
                .unwrap(),
            )
            .unwrap(),
            PageOperationOutcome::Succeeded(change),
            ObservationPart::Unavailable(KrometrailError::new(
                ErrorCode::PageObservationFailed,
                NonEmptyText::new("unavailable test observation").unwrap(),
            )),
        )
        .unwrap()
    }

    #[test]
    fn only_successful_explicit_navigation_results_mint_navigation_points() {
        let cases = [
            (
                BrowserOperationKind::NavigatePage,
                PageChange::Navigated,
                "navigate",
            ),
            (
                BrowserOperationKind::ReloadPage,
                PageChange::Reloaded,
                "reload",
            ),
            (BrowserOperationKind::GoBack, PageChange::WentBack, "back"),
            (
                BrowserOperationKind::GoForward,
                PageChange::WentForward,
                "forward",
            ),
        ];
        for (operation, change, label) in cases {
            let page = page_result(operation, change);
            let result = match operation {
                BrowserOperationKind::NavigatePage => {
                    BrowserOperationResult::NavigatePage(Box::new(page))
                }
                BrowserOperationKind::ReloadPage => {
                    BrowserOperationResult::ReloadPage(Box::new(page))
                }
                BrowserOperationKind::GoBack => BrowserOperationResult::GoBack(Box::new(page)),
                BrowserOperationKind::GoForward => {
                    BrowserOperationResult::GoForward(Box::new(page))
                }
                _ => unreachable!(),
            };
            assert!(
                project_result(&result, &FixedIds)
                    .unwrap()
                    .unwrap()
                    .navigation_id
                    .is_some(),
                "{label}"
            );
        }

        let selected = page_result(
            BrowserOperationKind::SelectPage,
            PageChange::Selected {
                previous: None,
                selected: TargetId::from_uuid(Uuid::from_u128(2)),
            },
        );
        assert!(
            project_result(
                &BrowserOperationResult::SelectPage(Box::new(selected)),
                &FixedIds,
            )
            .unwrap()
            .unwrap()
            .navigation_id
            .is_none()
        );
    }

    #[test]
    fn clipboard_write_evidence_persists_only_the_utf8_byte_count() {
        let operation = page_result(
            BrowserOperationKind::WriteClipboard,
            PageChange::ClipboardWritten,
        );
        let projection = project_result(
            &BrowserOperationResult::WriteClipboard(Box::new(
                krometrail_core::ClipboardWriteResult {
                    utf8_bytes: 17,
                    operation,
                },
            )),
            &FixedIds,
        )
        .unwrap()
        .unwrap();
        let record = projection.record.unwrap();
        assert_eq!(record.action, BrowserOperationKind::WriteClipboard);
        assert_eq!(
            record.sanitized_parameters.as_json(),
            &serde_json::json!({"utf8_bytes":17})
        );
        assert!(
            !serde_json::to_string(&record)
                .unwrap()
                .contains("clipboard text")
        );
    }

    #[test]
    fn download_cancellation_evidence_contains_only_opaque_id_and_state() {
        let operation = page_result(
            BrowserOperationKind::CancelDownload,
            PageChange::ClipboardWritten,
        )
        .interaction;
        let download_id = krometrail_core::DownloadId::from_uuid(Uuid::from_u128(44));
        let projection = project_result(
            &BrowserOperationResult::CancelDownload(Box::new(
                krometrail_core::CancelDownloadResult {
                    download_id,
                    state: krometrail_core::DownloadState::Cancelled,
                    operation,
                },
            )),
            &FixedIds,
        )
        .unwrap()
        .unwrap();
        let record = projection.record.unwrap();
        assert_eq!(record.action, BrowserOperationKind::CancelDownload);
        assert_eq!(
            record.sanitized_parameters.as_json(),
            &serde_json::json!({
                "download_id": download_id,
                "state": "cancelled",
            })
        );
        let encoded = serde_json::to_string(&record).unwrap();
        for forbidden in ["guid", "filename", "source_url", "resource_uri", "path"] {
            assert!(!encoded.contains(forbidden));
        }
    }
}
