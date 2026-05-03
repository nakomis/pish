# Pish — Pi Shell

<img src="docs/icon.png" alt="Pish icon — a raspberry thistle with widget tiles" width="120" align="right"/>

> A widget host for Raspberry Pi touch screens, written in Rust with [egui](https://github.com/emilk/egui).

Pish is a lightweight, iPad-style widget framework that runs on a Pi 5 + 7" touch display. Widgets are compiled in statically and rendered in a customisable grid. The first widgets are a digital clock, the [Nakostat](https://github.com/nakomis/nakostat) thermostat dial, and a BootBoots cat-camera slideshow.

---

## Support

If you find Pish useful, a small donation is very welcome:

[![Donate via PayPal](https://www.paypalobjects.com/en_GB/i/btn/btn_donate_LG.gif)](https://www.paypal.com/donate?hosted_button_id=Q3BESC73EWVNN&custom=pish)

---

## Widgets

| Widget | Description |
|--------|-------------|
| `clock` | Digital clock and date, drawn from a shared `Services` struct |
| `nakostat` | Thermostat rotary dial with MQTT setpoint (depends on `rotary-dial`) |
| `bootboots` | Cat-camera slideshow — Mu → Tau → Chi → Kappa, tap to advance |

## Repo layout

```
pish/
├── pish-core/        PishWidget trait, Services (WiFi, clock, brightness)
├── pish-shell/       eframe app — home screen grid, widget lifecycle
├── widgets/
│   ├── clock/
│   ├── nakostat/
│   └── bootboots/
├── githooks/         Git hooks (drawio→svg, README TOC)
└── scripts/          Helper scripts
```

## Running locally (macOS smoke test)

`eframe` runs natively on macOS. To see the full widget grid before deploying to the Pi:

```bash
./scripts/run-local.sh
# or directly:
cargo run
```

## Deploying to the Pi

Cross-compile and copy the binary:

```bash
./scripts/build-pi.sh
./scripts/deploy-pi.sh
```

Pish runs as a systemd service (`pish.service`) inside [cage](https://github.com/cage-kiosk/cage), a minimal Wayland kiosk compositor. It starts automatically on boot.

## Connecting via VNC

[wayvnc](https://github.com/any1/wayvnc) is installed and enabled as a systemd service (`wayvnc.service`). It attaches to cage's Wayland session automatically after `pish.service` starts.

To connect, open RealVNC Viewer and connect to `pish.local` (port 5900). You will be prompted for the Pi login password.

No manual steps are needed after a reboot — both `pish.service` and `wayvnc.service` start automatically.

## Building

```bash
cargo build --release
```

Cross-compile for Pi (aarch64):

```bash
cargo build --release --target aarch64-unknown-linux-gnu
```

## Testing

```bash
cargo test --workspace
cargo llvm-cov --workspace   # coverage report
```

CI requires ≥ 70 % line coverage.

## Adding a widget

1. `cargo new --lib widgets/<name>`
2. Add `pish-core = { workspace = true }` to its `Cargo.toml`.
3. Implement `PishWidget`.
4. Register it in `pish-shell/src/shell.rs`.
5. Add the new crate to the workspace `members` list.

## Licence

[CC0 1.0 Universal](LICENSE) — public domain dedication.

---

## Support

[![Donate via PayPal](https://www.paypalobjects.com/en_GB/i/btn/btn_donate_LG.gif)](https://www.paypal.com/donate?hosted_button_id=Q3BESC73EWVNN&custom=pish)
