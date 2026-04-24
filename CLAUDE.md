# Pish — Pi Shell

Widget host framework for Raspberry Pi touch screens, built with Rust/egui.

## Language

British English throughout — colour, licence, realise, whilst, etc.

## Branching & PRs

Always work on a feature branch, never directly on `main`. When implementation is complete, create a GitHub PR.

## Testing

This project adopts a **test-first approach** wherever practical:

- Write tests before or alongside implementation, not as an afterthought.
- Every non-trivial function, module, or component should have unit tests.
- Integration tests for interactions between crates (e.g. widget ↔ shell ↔ services).
- CI enforces a minimum **70% code coverage** across the workspace. Aim higher.
- Run tests: `cargo test --workspace`
- Run coverage: `cargo llvm-cov --workspace`
- PRs cannot be merged to `main` unless all CI checks pass. Admins may bypass in emergencies.

## Architecture

```
pish/
├── pish-core/        PishWidget trait, Services struct (WiFi, clock, brightness)
├── pish-shell/       eframe app — home screen grid, widget lifecycle
└── widgets/
    ├── clock/        Digital clock (uses services.clock)
    ├── nakostat/     Thermostat dial (rotary-dial crate, MQTT setpoint)
    └── bootboots/    Cat camera slideshow (Mu → Tau → Chi → Kappa)
```

Static compilation: all widgets are compiled in. No dynamic plugin loading.

The home screen is an iPad-style customisable grid. Each tile is 320×280 px minimum.

## Adding a widget

1. Create `widgets/<name>/` with a `Cargo.toml` depending on `pish-core`.
2. Implement `PishWidget` for your struct.
3. Add the crate to the workspace `Cargo.toml`.
4. Register an instance in `pish-shell/src/shell.rs`.

## Widgets in development

- **nakostat**: Thermostat dial — depends on `rotary-dial` (path dep during dev, crates.io once published).
- **bootboots**: Cat camera — currently static slideshow; TODO live BootBoots API.

## Running on Pi

Target: `aarch64-unknown-linux-gnu`. Cross-compile or build on Pi directly.

```
cargo build --release
```

Copy binary to Pi and run as a systemd service or from `/etc/xdg/autostart`.
