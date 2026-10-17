# SimRig

Configure your sim-racing rig on Linux — wheel base, steering wheel, pedals, shifter and
handbrake — whatever the brand, from one polished app.

> **Status: early development.** Milestone M0 (core model, plugin protocol and SDK, simulated
> rig, CLI) is done. The GUI comes next. Nothing here talks to real hardware yet.

## How it works

- **The app is a client of the drivers, just like a game.** It is never in the data path, and
  when it is closed it leaves no process running.
- **Drivers are independent.** They make devices work with the app closed, are installed when
  you add a device that needs them, and decide for themselves how they work.
- **The core defines standard interfaces** for each component type. Plugins implement them and
  report anything their hardware cannot do as *unsupported, with a reason*: the app shows it
  disabled with the explanation, never hides it.
- **Brand support comes from plugins**: external processes speaking a versioned JSON-RPC
  protocol, which may add their own self-described custom parameters.

Read more in [docs/requirements.md](docs/requirements.md) and
[docs/architecture.md](docs/architecture.md).

## Try it

Requires a recent stable Rust toolchain (1.86+).

```sh
cargo build --workspace
alias simrig-cli='./target/debug/simrig-cli --plugin-dir plugins --no-default-dirs'

simrig-cli list                                   # the simulated rig
simrig-cli show base                              # every item, unsupported ones with their reason
simrig-cli set base rotation 540
simrig-cli set shifter mode sequential            # watch `gear` become unavailable
simrig-cli show shifter
simrig-cli watch pedals                           # live inputs, Ctrl+C to stop
simrig-cli run base test_effect                   # asks for confirmation
simrig-cli check                                  # plugin conformance problems
```

Each command starts the plugins, talks to them and stops them: the simulated rig keeps its
settings only for the duration of a command.

## Repository layout

| Path | What |
|---|---|
| `crates/simrig-model` | Domain model: component interfaces, parameters, unsupported reasons, custom parameters |
| `crates/simrig-protocol` | App ↔ plugin protocol (JSON-RPC 2.0 over stdio) |
| `crates/simrig-plugin-sdk` | SDK to write plugin adapters in Rust |
| `crates/simrig-host` | Plugin discovery and processes, rig model, USB scan, conformance checks |
| `crates/simrig-cli` | Command-line interface |
| `plugins/sim` | Simulated rig plugin and end-to-end tests |

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Set `SIMRIG_PLUGIN_LOG=1` to see plugins' stderr.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
