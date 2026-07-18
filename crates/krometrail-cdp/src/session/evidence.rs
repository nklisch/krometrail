use krometrail_core::{
    BrowserOperationKind, BrowserOperationResult, ErrorCode, ErrorContext, IdSource,
    InteractionAnchor, InteractionEvidenceSink, InteractionRecord, KrometrailError, MonotonicClock,
    NavigationId, NonEmptyText, PageChange, PageOperationOutcome, Result, RetryAdvice,
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
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => Some(value.as_ref()),
        BrowserOperationResult::SetViewport(value) => Some(&value.operation),
        BrowserOperationResult::WriteClipboard(value) => Some(&value.operation),
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
        | BrowserOperationResult::CancelDownload(_)
        | BrowserOperationResult::Wait(_)
        | BrowserOperationResult::Batch(_) => None,
        BrowserOperationResult::CreatePage(_)
        | BrowserOperationResult::SelectPage(_)
        | BrowserOperationResult::ClosePage(_)
        | BrowserOperationResult::NavigatePage(_)
        | BrowserOperationResult::ReloadPage(_)
        | BrowserOperationResult::GoBack(_)
        | BrowserOperationResult::GoForward(_)
        | BrowserOperationResult::SetViewport(_)
        | BrowserOperationResult::WriteClipboard(_) => unreachable!("page results handled above"),
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
}
