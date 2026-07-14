# Browser target fixtures

These directories are standalone target applications for the Rust browser-control and temporal-evaluation foundations. They are not part of the Krometrail product runtime and do not import its implementation.

| Fixture | Current use |
| --- | --- |
| `page-lifecycle` | Dependency-free two-page target for page creation, selection, navigation, reload, same-document history, fallback, and closure qualification. |
| `page-observation` | Dependency-free structured-snapshot and screenshot target with dynamic replacement, disabled/hidden/inert controls, scrolling, shadow DOM, same-origin iframe content, and known CSS geometry. |
| `react-bugs` | Reproducible transient React-render and timer failures for screenshots, navigation, interaction, and temporal visual evidence. |
| `react-counter` | Minimal dynamic DOM and state-change target for browser actions and before/during/after capture. |
| `react-spa` | Multi-route target with forms, async data, dynamic lists, and transient visual defects for control and temporal-evaluation scenarios. |
| `simple-page` | Dependency-free baseline for navigation, forms, DOM changes, console/error evidence, and screenshots. |
| `verified-interactions` | Dependency-free target for reference/selector/coordinate input, forms, drag, scrolling, upload, dialogs, explicit no-op boundaries, and post-action evidence. |
| `waits-and-batches` | Dependency-free two-page target for delayed text/element/page state, navigation readiness, finite-network and long-lived-connection limitations, stale references, and ordered batch qualification. |
| `test-app` | Multi-page navigation, login/settings forms, validation failures, delayed responses, and WebSocket activity for browser-control and timeline capture. |
| `vue3-counter` | Minimal dynamic DOM and state-change target for browser actions and temporal capture. |
| `vue3-pinia` | Store-backed state changes and controls for browser interaction and temporal visual evidence. |
| `vue-bugs` | Reproducible transient Vue rendering and reactivity failures for visual capture and temporal evaluation. |
| `vue-spa` | Multi-route target with forms, async state, and transient visual defects for control and temporal-evaluation scenarios. |

The framework-specific fixtures are retained as target applications only: their current contract is what a browser renders and how it responds to browser actions, not framework-state inspection. Fixture servers and package metadata remain only where needed to launch the target application.
