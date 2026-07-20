---
id: idea-harden-frame-heavy-capture
created: 2026-07-20
updated: 2026-07-20
tags: [browser, testing]
---

Recheck capture acknowledgement and queue behavior during a frame-heavy GitHub issue search. In a temporary managed session with `every_nth_frame: 1`, navigating to `https://github.com/nklisch/krometrail/issues`, filling `Search Issues` with `viewport`, and pressing Enter produced 48 dropped frames out of 341 received and 54 known gaps. A temporal bundle around the search interaction retained five frames but crossed 37 gaps, so all three artifact outcomes were unavailable. Ordinary TodoMVC interaction and MDN scrolling in the same session produced usable gap-free evidence, making the heavy navigation path the useful stress reproduction.
