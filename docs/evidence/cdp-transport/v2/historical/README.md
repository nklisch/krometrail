# Historical schema-v2 qualification artifacts

These files were the prior v2 canonical reports and generated decision before the final strict requalification. On 2026-07-12 they were moved from `docs/evidence/cdp-transport/v2/` into this historical directory **without editing their bytes**. They remain available for audit and are not current qualification inputs.

| File | Original role | SHA-256 | Bytes |
| --- | --- | --- | ---: |
| [`cdpkit-linux.json`](./cdpkit-linux.json) | Prior accepted Linux v2 report from revision `3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2` | `0d11c4c8168d8ef2e988b2f71400696dc8a9521add23ba645b9ea65a03e0b148` | 6,951 |
| [`cdpkit-macos.json`](./cdpkit-macos.json) | Prior accepted hosted macOS v2 report, workflow run `29202919716`, same revision | `c206b1a04651421b8b88f42d75920800a75ee85ed83756f8792191a5e9b3b998` | 6,944 |
| [`decision.json`](./decision.json) | Prior generated v2 decision selecting `adopt_cdpkit` | `0288aa9a379b467042409ac27056107b443ea0d91bd21fc4fc8c2beae44c075b` | 13,546 |

The historical reports and decision are retained byte-for-byte. They are obsolete because the final requalification binds evidence to exact revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`, adds the repaired source attestation and final observed measurements, and requires a new decision. The current v2 directory contains only the final Linux and macOS reports; the replacement decision is deliberately deferred to the next story.
