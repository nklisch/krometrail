# Krometrail Skills

Two skills for AI agents, following the [Agent Skills specification](https://agentskills.io/specification).

## Install

```bash
# Both skills
npx skills add nklisch/krometrail --skill krometrail-debug krometrail-chrome

# Just the debugger
npx skills add nklisch/krometrail --skill krometrail-debug

# Just browser observation
npx skills add nklisch/krometrail --skill krometrail-chrome
```

## Skills

### krometrail-debug

Runtime debugging — breakpoints, stepping, variable inspection across 10 languages.

```
skills/krometrail-debug/
  SKILL.md
  references/
    cli.md          # Debug CLI commands
    python.md       # Python (debugpy)
    javascript.md   # JavaScript/TypeScript (js-debug)
    go.md           # Go (Delve)
    rust.md         # Rust (CodeLLDB)
    cpp.md          # C/C++ (GDB/LLDB)
    java.md         # Java (JDWP)
```

### krometrail-chrome

Browser observation — session recording, network/console/DOM/framework capture, investigation tools.

```
skills/krometrail-chrome/
  SKILL.md
  references/
    chrome.md       # Browser recording and investigation commands
```
