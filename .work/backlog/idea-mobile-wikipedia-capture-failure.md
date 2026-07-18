---
id: idea-mobile-wikipedia-capture-failure
created: 2026-07-17
updated: 2026-07-17
tags: [browser, visual]
---

Manual v1.0.4 testing reproducibly found retained capture entering `failed` at
`frame_envelope` after navigating to Wikipedia while a mobile viewport override is active. Current
browser control and live evidence continue to succeed, but subsequent operations are degraded and
temporal evidence is unavailable.

Observed twice with Chrome 150:

- start a temporary managed browser with `every_nth_frame: 2`;
- apply `360x640`, DPR 1, mobile/touch viewport metrics (also observed at `390x844`, DPR 3);
- navigate from a healthy page such as MDN to a Wikipedia article, or submit Wikipedia search
  navigation from one article to another;
- `browser_status` reports the target capture state as `failed`, `failure_stage: frame_envelope`.

Comparable mobile navigation to MDN and HTTPBin stayed healthy. One reproduction reported 32 frames
received, 25 accepted/persisted, 7 dropped, and 12 gaps. A later degraded response returned
correlation `b43455e9-699b-41c2-878e-b3646567a189`; its bounded diagnostic entries only recorded the
`capture_failed` response at `frame_envelope`, not the initiating failure detail.
