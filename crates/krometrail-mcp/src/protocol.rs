//! Qualified protocol policy; rmcp owns negotiation and wire projection.
use rmcp::model::{CacheScope, ProtocolVersion};
use rmcp::service::{RequestContext, RoleServer};

pub(crate) const VERSIONS: &[ProtocolVersion] = &[
    ProtocolVersion::V_2026_07_28,
    ProtocolVersion::V_2025_11_25,
    ProtocolVersion::V_2025_06_18,
];
pub(crate) const CATALOGUE_TTL_MS: u64 = 60_000;

pub(crate) fn modern(context: &RequestContext<RoleServer>) -> bool {
    context
        .protocol_version()
        .is_some_and(|v| v >= ProtocolVersion::V_2026_07_28)
}

pub(crate) fn cache_fields(modern: bool, ttl: u64) -> (Option<u64>, Option<CacheScope>) {
    if modern {
        (Some(ttl), Some(CacheScope::Private))
    } else {
        (None, None)
    }
}

pub(crate) fn no_cursor(
    request: &Option<rmcp::model::PaginatedRequestParams>,
) -> Result<(), rmcp::ErrorData> {
    if request.as_ref().and_then(|r| r.cursor.as_ref()).is_some() {
        Err(rmcp::ErrorData::invalid_params(
            "This inventory has no continuation; omit cursor.",
            None,
        ))
    } else {
        Ok(())
    }
}
