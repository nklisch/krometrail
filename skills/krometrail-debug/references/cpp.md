# C/C++ Debugging

## Prerequisites

One of:
- `gdb` v14+ (DAP support required): `apt-get install gdb` (Ubuntu)
- `lldb-dap` (fallback): `xcode-select --install` (macOS)

## Launch examples

```
# C source file (auto-compiles with gcc -g)
debug_launch({ command: "app.c" })

# C++ source file (auto-compiles with g++ -g)
debug_launch({ command: "main.cpp" })

# Pre-built binary
debug_launch({ command: "./myapp" })

# With make
debug_launch({ command: "make && ./myapp" })
```

## Attach to running process

Attach by PID:

```
debug_attach({ language: "cpp", pid: 12345 })
```

## Tips

- Source files are auto-compiled with debug symbols (`-g` flag) to `/tmp/`
- GDB v14+ is preferred; LLDB is used as fallback if GDB is too old
- For build systems (make/cmake), the binary path defaults to `./a.out` — specify the actual path for best results
- Uses stdin/stdout transport (unlike most other adapters which use TCP)
