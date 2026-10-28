# kru (कृ)

Tasks, organised in buckets. A command-line tool and a terminal UI in one
small binary.

## The name

kru comes from the Sanskrit root कृ (*kṛ*), "to do", pronounced "kru" in
many Indian languages. It is the root of words like *karma* (action) and
*kartavya* (what is to be done), which is what a todo list holds.

## Install

### Prebuilt binaries

Download the archive for your system from the
[latest release](https://github.com/SuhrudhSarathy/kru/releases/latest):

| System | Archive |
|---|---|
| macOS, Apple silicon | `kru-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `kru-x86_64-apple-darwin.tar.gz` |
| Linux, x86_64 | `kru-x86_64-unknown-linux-musl.tar.gz` |
| Linux, ARM64 | `kru-aarch64-unknown-linux-musl.tar.gz` |
| Windows, x86_64 | `kru-x86_64-pc-windows-msvc.zip` |

The Linux binaries are static and run on any distribution.

**macOS and Linux**: pick your archive's name from the table and install
`kru` into `~/.local/bin`:

```sh
target=aarch64-apple-darwin   # see the table above
mkdir -p ~/.local/bin
curl -fsSL "https://github.com/SuhrudhSarathy/kru/releases/latest/download/kru-$target.tar.gz" \
  | tar xz -C ~/.local/bin kru
```

Make sure `~/.local/bin` is on your `PATH`. If you downloaded the archive
with a browser on macOS and it refuses to run kru, clear the quarantine flag:
`xattr -d com.apple.quarantine ~/.local/bin/kru`.

**Windows**: unzip `kru-x86_64-pc-windows-msvc.zip` and put `kru.exe` in a
folder on your `PATH`.

Each archive has a `.sha256` file next to it in the release to check the
download against.

### From source

With [Rust](https://rustup.rs) 1.89 or newer:

```sh
cargo install --git https://github.com/SuhrudhSarathy/kru --locked
```

### Shell completion

Tab completes commands and flags, plus your bucket names, task numbers and
setting names (e.g. `kru task list -b <Tab>`). Add the line for your shell to
its startup file:

```sh
source <(COMPLETE=zsh kru)          # ~/.zshrc
source <(COMPLETE=bash kru)         # ~/.bashrc
COMPLETE=fish kru | source          # ~/.config/fish/config.fish
```

## Getting started

`kru --help` lists the commands; `kru` on its own opens the terminal UI.

To keep tasks in step across devices, create an empty private git repository
(e.g. on GitHub) and run `kru sync init <repo-url>` on each device. Syncing
needs `git` installed.

Agents and scripts: see [SKILL.md](SKILL.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in kru by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
