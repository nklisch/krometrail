# Java Debugging

## Prerequisites

- JDK 17+ (`javac` available): [Adoptium](https://adoptium.net)
- java-debug-adapter v0.53.0 (auto-downloaded to `~/.krometrail/adapters/java-debug/` on first use)
- Verify: `javac -version`

## Launch examples

```
# Main class
debug_launch({ command: "java Main" })

# With classpath
debug_launch({ command: "java -cp classes:lib/* com.example.App" })

# JAR file
debug_launch({ command: "java -jar app.jar" })
```

## Attach to running process

Start the JVM with JDWP agent:

```bash
java -agentlib:jdwp=transport=dt_socket,server=y,suspend=y,address=5005 Main
```

Then:

```
debug_attach({ language: "java", port: 5005 })
```

## Tips

- Classpath is automatically parsed from `-cp` / `-classpath` arguments
- Default classpath is `.` (current directory) if not specified
- Default JDWP port for attach: `5005`
- For attach, the JVM must have the JDWP agent enabled before starting
