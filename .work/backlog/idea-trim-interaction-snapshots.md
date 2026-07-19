---
id: idea-trim-interaction-snapshots
created: 2026-07-18
updated: 2026-07-18
tags: [agent-ux, browser]
---

The new `interaction_only` projection chooses the important controls correctly but can still consume a large agent payload because it returns complete `SnapshotNode` objects plus every required ancestor. In the post-1.1.2 public-site pass, Wikipedia's deeply nested focused search field was retained ahead of footer links, and Hacker News's bottom-page textbox survived 230 earlier links, so the ranking improvement works. However, `krometrail.dev` still returned the full 48-node budget with 336 omitted nodes, while Hacker News returned 48 nodes with 1,578 omitted nodes. The results included detailed properties for every node and long chains of layout-table/generic ancestors; focusable root, heading, and generic nodes also appeared as actions. This makes the smallest targeting projection materially more verbose than the line-oriented in-app and Chrome snapshots even when the desired control is found immediately.
