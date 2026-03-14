# Debug CLI Command Reference

All commands follow the pattern: `krometrail <command> [options]`

## Session management

```
krometrail launch "<cmd>" [--break <bp>] [--stop-on-entry] [--language <lang>]
krometrail attach --language <lang> [--port <n>] [--pid <n>]
krometrail stop [--session <id>]
krometrail status [--session <id>]
```

## Execution control

```
krometrail continue [--timeout <ms>]
krometrail step over|into|out [--count <n>]
krometrail run-to <file>:<line> [--timeout <ms>]
```

## Breakpoints

```
krometrail break <file>:<line>[,<line>,...] [when <cond>] [hit <cond>] [log '<msg>']
krometrail break --exceptions <filter>
krometrail break --clear <file>
krometrail breakpoints
```

### Conditional breakpoint examples

```
krometrail break "cart.py:42 when discount < 0"
krometrail break "loop.py:10 hit >=100"
krometrail break "app.py:30 log 'total={total}, items={len(items)}'"
```

## Inspection

```
krometrail eval "<expr>" [--frame <n>] [--depth <n>]
krometrail vars [--scope local|global|closure|all] [--filter "<regex>"]
krometrail stack [--frames <n>] [--source]
krometrail source <file>[:<start>-<end>]
krometrail watch "<expr>" ["<expr>" ...]
krometrail unwatch "<expr>"
```

## Session history and output

```
krometrail log [--detailed]
krometrail output [--stderr|--stdout] [--since-action <n>]
krometrail threads
```

## Diagnostics

```
krometrail doctor
```
