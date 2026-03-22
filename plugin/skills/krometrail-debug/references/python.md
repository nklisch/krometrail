# Python Debugging

## Prerequisites

- `python3` with `debugpy` installed: `pip install debugpy`
- Verify: `python3 -m debugpy --version`

## Launch examples

```
# Script
debug_launch({ command: "python3 app.py" })

# pytest (auto-detected)
debug_launch({ command: "python3 -m pytest tests/ -x" })

# Module mode
debug_launch({ command: "python3 -m mypackage.main" })

# Flask (auto-detected)
debug_launch({ command: "python3 -m flask run" })

# Django (auto-detected)
debug_launch({ command: "python3 manage.py runserver" })
```

## Attach to running process

The target must be running with debugpy listening:

```bash
python3 -m debugpy --listen 5678 app.py
```

Then:
```
debug_attach({ language: "python", port: 5678 })
```

## Exception breakpoints

| Filter | Breaks on |
|--------|-----------|
| `raised` | All exceptions (including caught) |
| `uncaught` | Only unhandled exceptions |
| `userUnhandled` | Exceptions not handled in user code |

```
debug_set_exception_breakpoints({ session_id: "...", filters: ["uncaught"] })
```

## Conditional breakpoints

Python expressions are supported:

```
debug_set_breakpoints({
  session_id: "...",
  file: "cart.py",
  breakpoints: [
    { line: 42, condition: "discount < 0" },
    { line: 55, condition: "len(items) > 100" },
    { line: 60, hitCondition: ">=10" },
    { line: 70, logMessage: "discount={discount}, total={total}" }
  ]
})
```

## Tips

- Logpoints use `{expression}` syntax for interpolation
- pytest `-x` flag stops on first failure — useful for debugging one test at a time
- For Django/Flask, set breakpoints in view functions before making the request
