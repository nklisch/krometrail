---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-final-v3-rollup
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-final-requalification]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Regenerate and roll forward the final strict decision

## Scope

Generate and install the final platform-faithful decision solely from accepted post-review reports. Roll exact revision, report/fixture/trace/source-attestation digests, exact post-receive acknowledgement measurements, selection, limitations, and provenance through evidence docs, research, skill, feature, epic, architecture, and stale narratives. Keep the workflow manual exact-ref+SHA only with resolved-SHA artifacts and no push trigger. Reproduce all default/spike/candidate and docs gates. Remote branch deletion is an authorized parent action and is intentionally outside this local story.

## Acceptance criteria

- [x] Decision bytes derive solely from both then-current reports, authenticate exact fixtures, identical trace evidence, source attestation, and clean-tree provenance; the superseded generated decision SHA-256 is `91f9032315dd3501068e1dd692b12fbda7ce0d7a57c9b5a49444db73c2a5c015`.
- [x] This prior rollup agrees on exact revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`, exact `cdpkit` 0.4.0, report/fixture/trace digests, platform measurements, receive → ack completion → bounded handoff order, limitations, and historical status; final5 superseded it.
- [x] The local workflow remains manual exact-ref+SHA only with resolved-SHA artifacts and no cdp-transport push trigger; no production/core leakage landed.
- [x] Default, spike, cdpkit, and docs build gates pass.
- [x] Parent deleted remote `ci/cdp-macos-evidence` as the separately authorized external action; `git ls-remote` confirms it is absent.

## Local implementation notes

- Installed `docs/evidence/cdp-transport/v2/decision.json` by running `decide_from_files` against only the then-current Linux and macOS reports. The superseded report digests were Linux `a7195eda1667e613b1b3f857fd56cc60153500544493a86afac8448706d20270` and macOS `46901e41bb2a4bb674d76d9dce41fc4200032280cd9720daaaad965ee89d257b`; hosted macOS run `29207244853`.
- The superseded reports bound revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`, source-attestation digest `sha256:b4147b12577e980123bfb711d314dd17f22b0639303956e97441af74a8b297b0`, configuration digest `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e`, browser fixture digest `sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13`, candidate trace digest `sha256:6c6be028c511d4d8c28cbecec368a7d4f09e0d87612741d02ac19a8663964d54`, and 942 observations. They are preserved byte-for-byte under `historical/final-v2-07b0990/`.
- The superseded acknowledgement contract was receive → ack completion → bounded handoff. Linux: 3,601 frames / 60.012037205 s, ack p99/max `0.3979589999999999/2.785427` ms; macOS: 3,566 frames / 60.011273167 s, ack p99/max `1.062666/7.058083` ms. Final5 replaced those current claims with its independently validated report and decision bytes; its ack metrics likewise begin after the frame is returned and exclude receive wait and later handoff.
- Limitations remain explicit: named event params rather than wildcard/full-envelope receive; unbounded cdpkit subscriber with no queue-depth introspection; RSS process-level proxy; candidate-contract trace is scripted evidence rather than a real-Chrome drift measurement. Selection is exact `cdpkit` 0.4.0; chromey and owned transport remain fallback reasoning only after demonstrated failure.
- Verification: the prior default/spike/cdpkit full gates, formatting, and docs build passed. `.work/bin/work-view` was restored; `.pi/` was ignored. Remote branch deletion was an authorized parent action and is not evidence for this local final5 story.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane final-rollup review reproduced decision bytes/digest from only the canonical reports, verified exact attestation/report/fixture/trace identity and platform measurements across authoritative docs, and confirmed manual-only workflow plus absent temporary remote branch. Verdict: Approve - story verified by implement; fast-lane advance.
