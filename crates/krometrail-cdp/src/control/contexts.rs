use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
};

use krometrail_core::{
    ErrorCode, FrameAccess, MAX_PAGE_ASSETS, MAX_PAGE_FRAMES, NonEmptyText, PageAssetInventory,
    PageAssetKind, PageAssetMetadata, PageFrameInventory, PageFrameReference, PageFrameStatus,
    Result, SanitizedUrl,
};
use serde_json::{Value, json};
use url::Url;

use super::{BoundTarget, PageControl, operation_error, transport_error};
use crate::transport::{CdpTransport, CommandScope};

const RESOURCE_TIMING_EXPRESSION: &str = r#"(() => { const all = performance.getEntriesByType('resource'); const valid = all.map((e) => ({name:e.name,initiatorType:e.initiatorType,startTime:e.startTime,duration:e.duration,transferSize:e.transferSize,encodedBodySize:e.encodedBodySize,decodedBodySize:e.decodedBodySize})).filter((e) => typeof e.name === 'string' && Number.isFinite(e.startTime) && e.startTime >= 0 && Number.isFinite(e.duration) && e.duration >= 0).sort((a,b) => a.startTime-b.startTime || a.name.localeCompare(b.name)); const entries = valid.slice(0, 256); return {entries, omitted: all.length - entries.length}; })()"#;

impl PageControl {
    pub(super) async fn resolve_frame_id(
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        reference: &PageFrameReference,
    ) -> Result<(String, String)> {
        if reference.target_id != bound.target_id
            || reference.attachment_generation != bound.attachment_generation
        {
            return Err(operation_error(
                ErrorCode::StaleReference,
                bound.target_id,
                "frame reference does not belong to the current page attachment",
            ));
        }
        let response = transport
            .send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Page.getFrameTree",
                json!({}),
            )
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
            })?;
        let root = response.get("frameTree").unwrap_or(&response);
        let root_origin = frame_url(root).and_then(|url| inherited_origin(&url, None));
        let oopif_ids = oopif_frame_ids(transport).await;
        let (frame, frame_origin) =
            find_frame_context(root, reference.frame_key.as_str(), root_origin.as_ref())
                .ok_or_else(|| {
                    operation_error(
                        ErrorCode::StaleReference,
                        bound.target_id,
                        "frame reference is no longer present",
                    )
                })?;
        let frame_id = frame
            .pointer("/frame/id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                operation_error(
                    ErrorCode::PageObservationFailed,
                    bound.target_id,
                    "browser returned an invalid frame identity",
                )
            })?;
        if !std::ptr::eq(frame, root) {
            match oopif_ids.as_ref() {
                Some(ids) if ids.contains(frame_id) => {
                    return Err(operation_error(
                        ErrorCode::Unsupported,
                        bound.target_id,
                        "out-of-process frames cannot be inspected",
                    ));
                }
                None => {
                    return Err(operation_error(
                        ErrorCode::Unsupported,
                        bound.target_id,
                        "frame process qualification is indeterminate",
                    ));
                }
                Some(_) => {}
            }
        }
        if !std::ptr::eq(frame, root)
            && !same_frame_origin(root_origin.as_ref(), frame_origin.as_ref())
        {
            return Err(operation_error(
                ErrorCode::Unsupported,
                bound.target_id,
                "cross-origin or indeterminate frames cannot be inspected",
            ));
        }
        let loader_id = frame
            .pointer("/frame/loaderId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                operation_error(
                    ErrorCode::PageObservationFailed,
                    bound.target_id,
                    "browser returned an invalid frame document identity",
                )
            })?;
        Ok((frame_id.to_owned(), loader_id.to_owned()))
    }

    pub(super) async fn list_frames(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
    ) -> Result<PageFrameInventory> {
        let response = transport
            .send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Page.getFrameTree",
                json!({}),
            )
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
            })?;
        let root = response.get("frameTree").unwrap_or(&response);
        let root_url = frame_url(root).ok_or_else(|| {
            operation_error(
                ErrorCode::PageObservationFailed,
                bound.target_id,
                "browser returned an invalid frame tree",
            )
        })?;
        let root_origin = inherited_origin(&root_url, None);
        let oopif_ids = oopif_frame_ids(transport).await;
        let mut frames = Vec::new();
        let mut omitted = 0_u32;
        collect_frames(
            root,
            bound,
            None,
            0,
            root_origin.as_ref(),
            root_origin.as_ref(),
            oopif_ids.as_ref(),
            &mut frames,
            &mut omitted,
        )?;
        Ok(PageFrameInventory {
            target_id: bound.target_id,
            frames,
            omitted_frame_count: omitted,
        })
    }

    pub(super) async fn list_page_assets(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
    ) -> Result<PageAssetInventory> {
        let response = transport
            .send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Runtime.evaluate",
                resource_timing_params(),
            )
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
            })?;
        let projected = response
            .pointer("/result/result/value")
            .or_else(|| response.pointer("/result/value"))
            .or_else(|| response.get("value"))
            .ok_or_else(|| {
                operation_error(
                    ErrorCode::PageObservationFailed,
                    bound.target_id,
                    "browser returned invalid resource timing metadata",
                )
            })?;
        let values = projected
            .get("entries")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                operation_error(
                    ErrorCode::PageObservationFailed,
                    bound.target_id,
                    "browser returned invalid bounded resource timing metadata",
                )
            })?;
        let mut parsed = Vec::new();
        let mut omitted = projected
            .get("omitted")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| {
                operation_error(
                    ErrorCode::PageObservationFailed,
                    bound.target_id,
                    "browser returned invalid resource timing omission accounting",
                )
            })?;
        for value in values {
            match parse_asset(value) {
                Some(asset) => parsed.push(asset),
                None => omitted = omitted.saturating_add(1),
            }
        }
        parsed.sort_by(|left, right| {
            left.0.total_cmp(&right.0).then_with(|| {
                serde_json::to_string(&left.1.url)
                    .unwrap_or_default()
                    .cmp(&serde_json::to_string(&right.1.url).unwrap_or_default())
            })
        });
        if parsed.len() > MAX_PAGE_ASSETS {
            omitted = omitted.saturating_add((parsed.len() - MAX_PAGE_ASSETS) as u32);
            parsed.truncate(MAX_PAGE_ASSETS);
        }
        Ok(PageAssetInventory {
            target_id: bound.target_id,
            assets: parsed.into_iter().map(|(_, asset)| asset).collect(),
            omitted_asset_count: omitted,
        })
    }
}

fn resource_timing_params() -> Value {
    json!({
        "expression": RESOURCE_TIMING_EXPRESSION,
        "returnByValue": true,
        "awaitPromise": false,
        // The expression is fixed and adapter-owned. Chrome refuses Resource Timing reads when
        // asked to infer side effects, so safety comes from the closed expression rather than
        // `throwOnSideEffect` heuristics.
        "throwOnSideEffect": false,
        "silent": true,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_frames(
    tree: &Value,
    bound: &BoundTarget,
    parent: Option<PageFrameReference>,
    depth: u16,
    root_origin: Option<&FrameOrigin>,
    parent_origin: Option<&FrameOrigin>,
    oopif_ids: Option<&HashSet<String>>,
    frames: &mut Vec<PageFrameStatus>,
    omitted: &mut u32,
) -> Result<()> {
    if frames.len() >= MAX_PAGE_FRAMES {
        *omitted = omitted.saturating_add(count_tree(tree) as u32);
        return Ok(());
    }
    let url = frame_url(tree).ok_or_else(|| {
        operation_error(
            ErrorCode::PageObservationFailed,
            bound.target_id,
            "browser returned an invalid frame URL",
        )
    })?;
    let frame_key = frame_token(tree).ok_or_else(|| {
        operation_error(
            ErrorCode::PageObservationFailed,
            bound.target_id,
            "browser returned an invalid frame identity",
        )
    })?;
    let reference = PageFrameReference {
        target_id: bound.target_id,
        attachment_generation: bound.attachment_generation,
        frame_key: NonEmptyText::new(frame_key).expect("generated frame key"),
    };
    let raw_frame_id = tree.pointer("/frame/id").and_then(Value::as_str);
    let frame_origin = inherited_origin(&url, parent_origin);
    let access = if depth == 0 {
        FrameAccess::MainDocument
    } else if oopif_ids.is_none() {
        FrameAccess::Indeterminate
    } else if raw_frame_id.is_some_and(|id| oopif_ids.is_some_and(|ids| ids.contains(id))) {
        FrameAccess::OutOfProcess
    } else if same_frame_origin(root_origin, frame_origin.as_ref()) {
        FrameAccess::SameOriginSameProcess
    } else if frame_origin.is_some() {
        FrameAccess::CrossOrigin
    } else {
        FrameAccess::Indeterminate
    };
    frames.push(PageFrameStatus {
        reference: reference.clone(),
        parent,
        depth,
        access,
        url: SanitizedUrl::sanitize(&url)?,
    });
    if let Some(children) = tree.get("childFrames").and_then(Value::as_array) {
        for child in children {
            collect_frames(
                child,
                bound,
                Some(reference.clone()),
                depth.saturating_add(1),
                root_origin,
                frame_origin.as_ref(),
                oopif_ids,
                frames,
                omitted,
            )?;
        }
    }
    Ok(())
}

async fn oopif_frame_ids(transport: &dyn CdpTransport) -> Option<HashSet<String>> {
    let response = transport
        .send_raw(&CommandScope::Browser, "Target.getTargets", json!({}))
        .await
        .ok()?;
    Some(
        response
            .get("targetInfos")?
            .as_array()?
            .iter()
            .filter(|target| target.get("type").and_then(Value::as_str) == Some("iframe"))
            .filter_map(|target| {
                target
                    .get("targetId")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect(),
    )
}

fn frame_url(tree: &Value) -> Option<String> {
    tree.pointer("/frame/url")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn frame_token(tree: &Value) -> Option<String> {
    let frame_id = tree.pointer("/frame/id")?.as_str()?;
    let loader_id = tree
        .pointer("/frame/loaderId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    frame_id.hash(&mut hasher);
    loader_id.hash(&mut hasher);
    Some(format!("frame-{:016x}", hasher.finish()))
}

fn origin(raw: &str) -> Option<String> {
    let parsed = Url::parse(raw).ok()?;
    Some(parsed.origin().ascii_serialization())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FrameOrigin {
    Tuple(String),
    Opaque { inherited_from_parent: bool },
}

fn inherited_origin(raw: &str, parent: Option<&FrameOrigin>) -> Option<FrameOrigin> {
    if matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "about:blank" | "about:srcdoc"
    ) {
        return parent.map(FrameOrigin::inherited);
    }
    let value = origin(raw)?;
    if value == "null" {
        Some(FrameOrigin::Opaque {
            inherited_from_parent: false,
        })
    } else {
        Some(FrameOrigin::Tuple(value))
    }
}

impl FrameOrigin {
    fn inherited(&self) -> Self {
        match self {
            Self::Tuple(value) => Self::Tuple(value.clone()),
            Self::Opaque { .. } => Self::Opaque {
                inherited_from_parent: true,
            },
        }
    }
}

fn same_frame_origin(root: Option<&FrameOrigin>, child: Option<&FrameOrigin>) -> bool {
    match (root, child) {
        (Some(FrameOrigin::Tuple(root)), Some(FrameOrigin::Tuple(child))) => root == child,
        (
            Some(FrameOrigin::Opaque { .. }),
            Some(FrameOrigin::Opaque {
                inherited_from_parent: true,
            }),
        ) => true,
        _ => false,
    }
}

fn count_tree(tree: &Value) -> usize {
    1 + tree
        .get("childFrames")
        .and_then(Value::as_array)
        .map(|children| children.iter().map(count_tree).sum())
        .unwrap_or(0)
}

fn find_frame_context<'a>(
    tree: &'a Value,
    token: &str,
    parent_origin: Option<&FrameOrigin>,
) -> Option<(&'a Value, Option<FrameOrigin>)> {
    let url = frame_url(tree)?;
    let effective_origin = inherited_origin(&url, parent_origin);
    if frame_token(tree).as_deref() == Some(token) {
        return Some((tree, effective_origin));
    }
    tree.get("childFrames")?
        .as_array()?
        .iter()
        .find_map(|child| find_frame_context(child, token, effective_origin.as_ref()))
}

fn parse_asset(value: &Value) -> Option<(f64, PageAssetMetadata)> {
    let start = finite_non_negative(value.get("startTime")?)?;
    let duration_ms = finite_non_negative(value.get("duration")?)?;
    let url = SanitizedUrl::sanitize(value.get("name")?.as_str()?).ok()?;
    let initiator_type = value
        .get("initiatorType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = classify_asset(&url, initiator_type);
    Some((
        start,
        PageAssetMetadata {
            url,
            kind,
            duration_ms,
            transfer_bytes: optional_size(value.get("transferSize")),
            encoded_body_bytes: optional_size(value.get("encodedBodySize")),
            decoded_body_bytes: optional_size(value.get("decodedBodySize")),
        },
    ))
}

fn classify_asset(url: &SanitizedUrl, initiator_type: &str) -> PageAssetKind {
    if let Some(kind) = url.extension().and_then(asset_kind_for_extension) {
        return kind;
    }
    match initiator_type {
        "script" => PageAssetKind::Script,
        "css" | "link" => PageAssetKind::Stylesheet,
        "img" => PageAssetKind::Image,
        "font" => PageAssetKind::Font,
        "audio" | "video" => PageAssetKind::Media,
        "fetch" => PageAssetKind::Fetch,
        "xmlhttprequest" => PageAssetKind::XmlHttpRequest,
        _ => PageAssetKind::Other,
    }
}

fn asset_kind_for_extension(extension: &str) -> Option<PageAssetKind> {
    match extension {
        "js" | "mjs" | "cjs" => Some(PageAssetKind::Script),
        "css" => Some(PageAssetKind::Stylesheet),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico" | "avif" => {
            Some(PageAssetKind::Image)
        }
        "woff" | "woff2" | "ttf" | "otf" => Some(PageAssetKind::Font),
        "mp3" | "mp4" | "webm" | "wav" => Some(PageAssetKind::Media),
        _ => None,
    }
}

fn finite_non_negative(value: &Value) -> Option<f64> {
    let value = value.as_f64()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn optional_size(value: Option<&Value>) -> Option<u64> {
    let value = finite_non_negative(value?)?;
    (value <= u64::MAX as f64).then_some(value as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound() -> BoundTarget {
        BoundTarget {
            target_id: krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(1)),
            browser_target_key: "page".into(),
            attachment_generation: 2,
            transport_session: crate::transport::TransportSessionId::new("session").unwrap(),
            visibility: krometrail_core::TargetVisibility::Visible,
        }
    }

    #[test]
    fn malformed_assets_are_omitted_and_urls_are_sanitized() {
        assert!(RESOURCE_TIMING_EXPRESSION.contains("slice(0, 256)"));
        assert!(RESOURCE_TIMING_EXPRESSION.contains("omitted"));
        assert_eq!(resource_timing_params()["throwOnSideEffect"], json!(false));
        let (_, asset) = parse_asset(&json!({
            "name":"https://example.test/app.js?token=secret#fragment",
            "initiatorType":"script","startTime":1.0,"duration":2.0,
            "transferSize":0,"encodedBodySize":12,"decodedBodySize":20
        }))
        .unwrap();
        assert_eq!(asset.kind, PageAssetKind::Script);
        assert_eq!(asset.transfer_bytes, Some(0));
        let serialized = serde_json::to_string(&asset).unwrap();
        assert!(!serialized.contains("secret"));
        assert!(
            parse_asset(&json!({"name":"https://x.test","startTime":-1,"duration":1})).is_none()
        );
    }

    #[test]
    fn asset_extensions_override_only_unambiguous_resource_identities() {
        let classify =
            |url, initiator| classify_asset(&SanitizedUrl::sanitize(url).unwrap(), initiator);
        assert_eq!(
            classify("https://example.test/module.MJS?cache=1#fragment", "link"),
            PageAssetKind::Script
        );
        assert_eq!(
            classify("https://example.test/font.WOFF2", "link"),
            PageAssetKind::Font
        );
        assert_eq!(
            classify("https://example.test/site.css", "css"),
            PageAssetKind::Stylesheet
        );
        assert_eq!(
            classify("https://example.test/hero.avif", "img"),
            PageAssetKind::Image
        );
        assert_eq!(
            classify("https://example.test/intro.webm", "video"),
            PageAssetKind::Media
        );
        assert_eq!(
            classify("https://example.test/resource", "fetch"),
            PageAssetKind::Fetch
        );
        assert_eq!(
            classify("https://example.test/api.json", "xmlhttprequest"),
            PageAssetKind::XmlHttpRequest
        );
    }

    #[test]
    fn frame_tokens_are_opaque_and_loader_scoped() {
        let first = json!({"frame":{"id":"raw-frame-id","loaderId":"loader-a","url":"https://example.test/frame"}});
        let navigated = json!({"frame":{"id":"raw-frame-id","loaderId":"loader-b","url":"https://example.test/frame"}});
        let first_token = frame_token(&first).unwrap();
        assert!(first_token.starts_with("frame-"));
        assert!(!first_token.contains("raw-frame-id"));
        assert_ne!(first_token, frame_token(&navigated).unwrap());
    }

    #[test]
    fn frame_inventory_distinguishes_same_origin_cross_origin_and_oopif() {
        let tree = json!({
            "frame":{"id":"root","loaderId":"root-loader","url":"https://example.test/"},
            "childFrames":[
                {"frame":{"id":"same","loaderId":"same-loader","url":"https://example.test/frame"}},
                {"frame":{"id":"cross","loaderId":"cross-loader","url":"https://other.test/frame"}},
                {"frame":{"id":"oopif","loaderId":"oopif-loader","url":"https://example.test/isolated"}},
                {"frame":{"id":"blank","loaderId":"blank-loader","url":"about:blank"}},
                {"frame":{"id":"opaque","loaderId":"opaque-loader","url":"data:text/html,opaque"}}
            ]
        });
        let oopif = HashSet::from(["oopif".to_owned()]);
        let mut frames = Vec::new();
        let mut omitted = 0;
        let root_origin = FrameOrigin::Tuple("https://example.test".to_owned());
        collect_frames(
            &tree,
            &bound(),
            None,
            0,
            Some(&root_origin),
            Some(&root_origin),
            Some(&oopif),
            &mut frames,
            &mut omitted,
        )
        .unwrap();
        assert_eq!(omitted, 0);
        assert_eq!(frames[0].access, FrameAccess::MainDocument);
        assert_eq!(frames[1].access, FrameAccess::SameOriginSameProcess);
        assert_eq!(frames[2].access, FrameAccess::CrossOrigin);
        assert_eq!(frames[3].access, FrameAccess::OutOfProcess);
        assert_eq!(frames[4].access, FrameAccess::SameOriginSameProcess);
        assert_eq!(frames[5].access, FrameAccess::CrossOrigin);
    }

    #[test]
    fn frame_inventory_qualifies_only_inherited_same_process_opaque_origins() {
        let tree = json!({
            "frame":{"id":"root","loaderId":"root-loader","url":"data:text/html,root"},
            "childFrames":[
                {"frame":{"id":"srcdoc","loaderId":"srcdoc-loader","url":"about:srcdoc"}},
                {"frame":{"id":"data","loaderId":"data-loader","url":"data:text/html,child"}}
            ]
        });
        let oopif = HashSet::new();
        let mut frames = Vec::new();
        let mut omitted = 0;
        let root_origin = inherited_origin("data:text/html,root", None).unwrap();
        collect_frames(
            &tree,
            &bound(),
            None,
            0,
            Some(&root_origin),
            Some(&root_origin),
            Some(&oopif),
            &mut frames,
            &mut omitted,
        )
        .unwrap();

        assert_eq!(frames[0].access, FrameAccess::MainDocument);
        assert_eq!(frames[1].access, FrameAccess::SameOriginSameProcess);
        assert_eq!(frames[2].access, FrameAccess::CrossOrigin);
        let (_, srcdoc_origin) = find_frame_context(
            &tree,
            frame_token(&tree["childFrames"][0]).unwrap().as_str(),
            Some(&root_origin),
        )
        .unwrap();
        let (_, data_origin) = find_frame_context(
            &tree,
            frame_token(&tree["childFrames"][1]).unwrap().as_str(),
            Some(&root_origin),
        )
        .unwrap();
        assert!(same_frame_origin(
            Some(&root_origin),
            srcdoc_origin.as_ref()
        ));
        assert!(!same_frame_origin(Some(&root_origin), data_origin.as_ref()));
        assert!(same_frame_origin(
            Some(&FrameOrigin::Opaque {
                inherited_from_parent: false,
            }),
            Some(&FrameOrigin::Opaque {
                inherited_from_parent: true,
            })
        ));
        assert!(!same_frame_origin(
            Some(&FrameOrigin::Opaque {
                inherited_from_parent: false,
            }),
            Some(&FrameOrigin::Opaque {
                inherited_from_parent: false,
            })
        ));
    }
}
