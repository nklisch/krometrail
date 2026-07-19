---
id: idea-frame-heavy-ack-regression
created: 2026-07-18
updated: 2026-07-18
tags: [browser, testing]
---

Released Krometrail 1.2.0 still terminally failed retained capture at the acknowledgement stage during manual navigation to `https://www.w3schools.com/html/html_iframe.asp`. The managed session remained ready for browser control, but concise status reported capture `state: failed`, 219 received frames, 199 persisted frames, 19 dropped frames, and 28 known gaps. The failure reproduces the frame-heavy class addressed by `runtime-observation-hardening-capture-acknowledgements`; inspect the privacy-bounded diagnostic log for the actual acknowledgement reason and elapsed/deadline facts before choosing a new fix.
