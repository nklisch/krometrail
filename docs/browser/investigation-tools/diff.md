---
title: Diff
description: Your agent can compare two moments in your session — what changed between working and broken.
---

# Diff

Your agent can compare two moments in your session — what changed between "working" and "broken". This is often the fastest path to identifying what went wrong.

## How Markers Enable This

When you place markers at key moments during recording, your agent can use them as reference points for comparison. If you mark "before form submit" and "after error appeared", your agent diffs everything that happened between those two moments — network activity, storage changes, console output, and component state.

You don't need to understand the bug to place useful markers. Just mark the moment before you trigger the action and the moment after something looks wrong, and your agent does the rest.

## What Gets Compared

**URL changes** — the navigation sequence between the two timestamps, showing which pages were visited.

**Storage diffs** — localStorage and sessionStorage changes: keys added, removed, or modified with their old and new values.

**Network summary** — requests made in the window, grouped by status code. Highlights new failures that appeared between the two points.

**Console changes** — new console errors or warnings that appeared between the timestamps.

**Framework state changes** — component mount/unmount events and state changes, showing which React or Vue components changed during the window.

**Screenshot comparison** — nearest screenshots at each timestamp for visual reference.

## Example: Diagnosing a Form Submission Bug

Suppose you're testing a checkout form and the payment silently fails. During recording, you click **◎ Mark** right before you click "Place Order", and again when you see the error appear. You label them "form submitted" and "error displayed".

Your agent then diffs the state between those two markers. The diff output shows which network request failed and its response body, whether localStorage was modified unexpectedly, which React components re-rendered and with what state changes, and any console errors logged in that window.

This is often enough to identify the root cause without inspecting individual events one by one.
