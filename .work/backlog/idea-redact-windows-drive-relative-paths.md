---
id: idea-redact-windows-drive-relative-paths
created: 2026-07-14
updated: 2026-07-14
tags: [security, browser]
---

Extend the browser-event text redactor's path corpus to cover Windows drive-relative forms such as `C:foo` and rooted single-backslash paths such as `\Users\alice\file`. Chrome usually emits caught `file:///` or `C:\...` forms, so this is defense in depth rather than a demonstrated active leak. Add table-driven cases without weakening current URL, credential, query, fragment, POSIX, UNC, and absolute-drive redaction.
