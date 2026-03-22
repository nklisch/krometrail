# Python Debugging (debugpy)

## Prerequisites
```sh
pip install debugpy
krometrail doctor  # verifies debugpy is available
```

## Launch
```
debug_launch(command: 'python app.py')
debug_launch(command: 'python -m app')
debug_launch(command: 'python app.py --port 8080')
```

Language is auto-detected from the command. Override with `language: 'python'` if needed.

## Frameworks (auto-detected)

### pytest
```
debug_launch(command: 'pytest tests/test_order.py -s')
debug_launch(command: 'pytest tests/test_order.py::TestCart::test_discount -s')
```
`-s` disables output capture so stdout is visible in `debug_output`.

### Django / Flask / FastAPI
```
debug_launch(command: 'python manage.py runserver --noreload')
debug_launch(command: 'uvicorn app:app --reload')
```
Use `--noreload` / disable reloading to prevent the debugger from losing the process.

## Attach (debugpy remote)
Start the process with:
```sh
python -m debugpy --listen 5678 --wait-for-client app.py
```
Then:
```
debug_attach(language: 'python', port: 5678)
```

## Exception Breakpoints
```
debug_set_exception_breakpoints(session_id: '...', filters: ['raised'])       # all exceptions
debug_set_exception_breakpoints(session_id: '...', filters: ['uncaught'])     # unhandled only
debug_set_exception_breakpoints(session_id: '...', filters: ['userUnhandled'])
```

## Variable Scopes
- `local` — current function locals (default)
- `global` — module-level globals
- `closure` — not available in Python

## Known Quirks
- Breakpoints on decorator lines move to the first body line — this is normal.
- f-string expressions evaluate fine with `debug_evaluate`.
- `__dict__` is useful for inspecting object state: `debug_evaluate(expression: 'obj.__dict__')`.
- Django ORM querysets are lazy — call `list(qs)` in `debug_evaluate` to force evaluation.
