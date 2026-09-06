//! Immutable pages projected from the configured router, not a second registry.
use crate::protocol::{CATALOGUE_TTL_MS, cache_fields};
use krometrail_core::Sha256Digest;
use rmcp::{
    ErrorData,
    model::{ListToolsResult, Tool},
};

const MAX_TOOLS: usize = 8;
const MAX_BYTES: usize = 192 * 1024;
const MAX_CURSOR_BYTES: usize = 160;

pub(crate) struct Catalogue {
    tools: Vec<Tool>,
    pages: Vec<(usize, usize)>,
    prefix: String,
    error: Option<ErrorData>,
}

impl Catalogue {
    pub(crate) fn new(mut tools: Vec<Tool>) -> Self {
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        let digest = Sha256Digest::digest(
            &serde_json::to_vec(&(MAX_TOOLS, MAX_BYTES, &tools))
                .expect("tool descriptors serialize"),
        );
        let prefix = format!("tools-v1.{}.{}.", uuid::Uuid::new_v4(), digest);
        let mut catalogue = Self {
            tools,
            pages: Vec::new(),
            prefix,
            error: None,
        };
        let mut start = 0;
        while start < catalogue.tools.len() {
            let mut end = start;
            while end < catalogue.tools.len() && end - start < MAX_TOOLS {
                let candidate = catalogue.result(start, end + 1, true);
                let bytes = serde_json::to_vec(&candidate)
                    .expect("catalogue result serializes")
                    .len();
                if bytes > MAX_BYTES {
                    if end == start {
                        catalogue.error = Some(ErrorData::internal_error(
                            format!(
                                "Registered tool {} requires {bytes} catalogue bytes; reduce its descriptor to fit {MAX_BYTES} bytes.",
                                catalogue.tools[start].name
                            ),
                            None,
                        ));
                        return catalogue;
                    }
                    break;
                }
                end += 1;
            }
            catalogue.pages.push((start, end));
            start = end;
        }
        catalogue
    }

    #[cfg(test)]
    pub(crate) fn tools(&self) -> &[Tool] {
        &self.tools
    }
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.tools
            .binary_search_by(|tool| tool.name.as_ref().cmp(name))
            .is_ok()
    }
    fn result(&self, start: usize, end: usize, modern: bool) -> ListToolsResult {
        let mut result = ListToolsResult::with_all_items(self.tools[start..end].to_vec());
        if end < self.tools.len() {
            result.next_cursor = Some(format!("{}{end}", self.prefix));
        }
        (result.ttl_ms, result.cache_scope) = cache_fields(modern, CATALOGUE_TTL_MS);
        if !modern {
            result.result_type = None;
        }
        result
    }
    pub(crate) fn page(
        &self,
        cursor: Option<&str>,
        modern: bool,
    ) -> Result<ListToolsResult, ErrorData> {
        // Supported legacy hosts do not reliably follow continuation cursors.
        // Keep one catalogue authority, projected by the resolved wire version.
        if !modern {
            if cursor.is_some() {
                return Err(invalid_cursor());
            }
            return Ok(self.result(0, self.tools.len(), false));
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let start = if let Some(cursor) = cursor {
            if cursor.len() > MAX_CURSOR_BYTES || !cursor.is_ascii() {
                return Err(invalid_cursor());
            }
            let index = cursor
                .strip_prefix(&self.prefix)
                .ok_or_else(invalid_cursor)?;
            let start: usize = index.parse().map_err(|_| invalid_cursor())?;
            if start == 0 || start.to_string() != index || !self.pages.iter().any(|p| p.0 == start)
            {
                return Err(invalid_cursor());
            }
            start
        } else {
            0
        };
        if self.tools.is_empty() {
            return Ok(self.result(0, 0, modern));
        }
        let (_, end) = self
            .pages
            .iter()
            .find(|p| p.0 == start)
            .ok_or_else(invalid_cursor)?;
        Ok(self.result(start, *end, modern))
    }
}
fn invalid_cursor() -> ErrorData {
    ErrorData::invalid_params(
        "Invalid catalogue cursor; discard cached pages and restart tools/list without a cursor.",
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tools(count: usize) -> Vec<Tool> {
        (0..count)
            .map(|i| {
                Tool::new(
                    format!("tool_{i:03}"),
                    "fixture",
                    std::sync::Arc::new(
                        serde_json::json!({"type":"object"})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
            })
            .collect()
    }
    #[test]
    fn traversal_is_exact_bounded_repeatable_and_process_scoped() {
        let catalogue = Catalogue::new(tools(23));
        let foreign = Catalogue::new(tools(23));
        let mut cursor = None;
        let mut names = Vec::new();
        loop {
            let page = catalogue.page(cursor.as_deref(), true).unwrap();
            assert!(page.tools.len() <= MAX_TOOLS);
            assert!(serde_json::to_vec(&page).unwrap().len() <= MAX_BYTES);
            assert_eq!(
                serde_json::to_value(&page).unwrap(),
                serde_json::to_value(catalogue.page(cursor.as_deref(), true).unwrap()).unwrap()
            );
            names.extend(page.tools.iter().map(|t| t.name.to_string()));
            cursor = page.next_cursor;
            if let Some(c) = &cursor {
                assert!(foreign.page(Some(c), true).is_err());
                assert!(catalogue.page(Some(c), false).is_err());
            } else {
                break;
            }
        }
        assert_eq!(
            names,
            catalogue
                .tools
                .iter()
                .map(|t| t.name.to_string())
                .collect::<Vec<_>>()
        );
        let legacy = catalogue.page(None, false).unwrap();
        assert_eq!(legacy.tools, catalogue.tools);
        assert_eq!(legacy.tools, catalogue.page(None, false).unwrap().tools);
        assert!(legacy.next_cursor.is_none());
        assert!(legacy.ttl_ms.is_none());
        assert!(legacy.cache_scope.is_none());
        assert!(catalogue.page(Some(""), false).is_err());
        assert!(
            Catalogue::new(vec![])
                .page(None, false)
                .unwrap()
                .tools
                .is_empty()
        );
        for index in [
            "0",
            "1",
            "23",
            "024",
            "99999999999999999999999999999999999999",
            "8.extra",
        ] {
            assert!(
                catalogue
                    .page(Some(&format!("{}{index}", catalogue.prefix)), true)
                    .is_err()
            );
        }
        assert!(catalogue.page(Some(&"a".repeat(161)), true).is_err());
        assert!(
            Catalogue::new(vec![])
                .page(None, true)
                .unwrap()
                .tools
                .is_empty()
        );
    }
    #[test]
    fn oversized_descriptor_is_a_list_error_not_startup_failure() {
        let mut tools = tools(1);
        tools[0].description = Some("x".repeat(MAX_BYTES).into());
        let catalogue = Catalogue::new(tools);
        assert!(catalogue.page(None, true).is_err());
        assert!(catalogue.contains("tool_000"));
        assert_eq!(catalogue.page(None, false).unwrap().tools, catalogue.tools);
    }
}
