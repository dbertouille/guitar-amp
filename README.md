# guitar-amp

A real-time guitar amplifier in Rust. It reads audio from an input device (e.g.
your guitar interface), runs each sample through a `tanh` soft-clipping
distortion effect, and plays the result out an output device — continuously, with
as little latency as possible.

## Requirements

- A recent Rust toolchain (edition 2024 — Rust 1.85 or newer).

## Running

```sh
cargo run --release
```

You'll be prompted to pick an input and an output device (press Enter to accept
the default). Plug in your guitar, play, and listen. Press Ctrl+C to quit.

## Development

This repo ships a pre-commit hook (in [`.githooks/`](.githooks/)) that runs
`cargo fmt --all --check` and blocks the commit if anything is unformatted —
mirroring the CI format check. Git doesn't pick up the hook automatically, so
enable it once after cloning:

```sh
git config core.hooksPath .githooks
```

If formatting fails, run `cargo fmt --all` and re-stage.

## License

MIT — see [LICENSE](LICENSE).

<!-- TODO: add repository URL once the repo is published, e.g.
     https://github.com/<owner>/<repo> -->
