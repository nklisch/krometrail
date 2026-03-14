---
title: Python
description: Debug Python with debugpy. Supports pytest, Django, Flask, and plain scripts.
---

# Python

**Debugger:** [debugpy](https://github.com/microsoft/debugpy) (Microsoft)
**Status:** Stable
**Python version:** 3.8+

## Prerequisites

```bash
pip install debugpy
```

Verify: `python -m debugpy --version`

## Quick Start

```bash
# Debug a script
krometrail debug launch "python app.py" --break app.py:42

# Debug a pytest test
krometrail debug launch "python -m pytest tests/test_order.py::test_gold_discount -x" \
	--break order.py:147

# Debug Django
krometrail debug launch "python manage.py runserver" --break views.py:83

# Debug Flask
krometrail debug launch "flask run" --break routes.py:55
```

## Framework Auto-detection

pytest, Django, and Flask are auto-detected from the launch command. The adapter configures debugpy appropriately for each:

- **pytest** — disables test runner capture so breakpoints work correctly
- **Django** — enables Django template debugging, disables autoreload
- **Flask** — sets `WERKZEUG_RUN_MAIN=true` to prevent fork on autoreload

## Conditional Breakpoints

Python expressions work directly in conditions:

```bash
krometrail debug break "order.py:147 when discount < 0"
krometrail debug break "loop.py:25 when i == 99"
krometrail debug break "api.py:30 when request.method == 'POST'"
```

## Exception Breakpoints

```bash
krometrail debug break --exceptions uncaught    # only unhandled exceptions
krometrail debug break --exceptions raised      # all raised exceptions
```

For specific exception types, use a conditional breakpoint with `isinstance`.

## Tips

- The Python adapter uses `python -m debugpy --listen :PORT --wait-for-client` under the hood. You do not need to modify your code.
- `debug_evaluate` accepts any valid Python expression including method calls, comprehensions, and f-strings.
- Virtual environments are respected — the adapter uses the `python` found in your PATH or the one specified in the command.
- Use `--stop-on-entry` to pause on the first executable line (useful for understanding program structure).

## Troubleshooting

**Breakpoints not hit:**
- Confirm the file path is correct relative to `cwd`
- Check that `krometrail doctor` shows debugpy installed
- If using a virtual environment, ensure it's activated before launching

**`debugpy` import error in your program:**
- debugpy is only used by Krometrail's adapter subprocess, not imported into your code directly
