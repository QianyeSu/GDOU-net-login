# GDOU Net Login Desktop

Rust desktop client for the Guangdong Ocean University Srun campus network.

## Run

```bash
cargo run
```

The default command opens the GUI. CLI commands are still available for debugging:

```bash
cargo run -- status
cargo run -- login
cargo run -- watch --interval 30
```

## Build

```bash
cargo build --release
```

Windows output:

```text
target/release/gdou-net-login.exe
```
