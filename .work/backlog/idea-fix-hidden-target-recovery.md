---
id: idea-fix-hidden-target-recovery
created: 2026-07-18
updated: 2026-07-18
tags: []
---

In a managed `focus: preserve` session, opening Krometrail's public-site GitHub link created and visibly switched Chrome to a new tab while Krometrail kept the original documentation page logically selected. Scrolling the now-hidden original target correctly failed with `target_hidden`, but the recovery said `select or foreground the page, then retry the pointer operation`. In preserve mode, `select_page` is documented and observed as logical-only, and Krometrail exposes no foreground-page operation, so selecting the hidden target cannot satisfy the recovery. Align the error guidance with the actual focus policy and available operations, or provide an explicit user-authorized foreground action if that is intended.
