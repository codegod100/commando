# Commando

Unofficial Linux desktop client for [Desktop Commander](https://desktopcommander.app/). The official app ships for macOS and Windows only. Commando is a native Relm4 + libadwaita client that does the same job: describe an outcome, watch an agent work on your files and terminal.

This project is not affiliated with Desktop Commander.

## What it does

- Prompt bar, live action log, and workspace file browser
- Built-in prompt library, knowledge attachments, and file preview
- Local tools: list / read / write / edit / move / search files, run shell commands, and manage persistent interactive processes
- Models via Ollama (default), OpenAI, a ChatGPT subscription, Anthropic, OpenRouter, or any OpenAI-compatible server

## Requirements

- Rust 1.85+
- GTK 4 and libadwaita
- A model endpoint. The fastest local path is [Ollama](https://ollama.com/) with any chat model, for example `ollama pull llama3.2`

On Fedora:

```bash
sudo dnf install gtk4-devel libadwaita-devel gcc
```

On Debian / Ubuntu:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev build-essential
```

## Run

```bash
cargo run --release
```

Settings live in `~/.config/commando/config.toml` (mode `600` because it can hold an API key). Do not commit that file.

## Flatpak

Build and install the local Flatpak with:

```bash
flatpak-builder --force-clean --user --install build-dir app.commando.Commando.yml
flatpak run app.commando.Commando
```

The manifest uses the GNOME 49 runtime and downloads the dependencies pinned in
`Cargo.lock` while building. Install the SDK first if it is not already
available:

```bash
flatpak install flathub org.gnome.Platform//49 org.gnome.Sdk//49 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08
```

To use a ChatGPT Plus, Pro, Business, Edu, or Enterprise subscription, install
the OpenAI Codex CLI and run `codex login`, then select **ChatGPT subscription**
in Settings. Commando reuses the local Codex login; no API key is required.

## Safety

The agent runs real commands as your user. A small blocklist stops obvious accidents (`rm -rf /`, `mkfs`, reboot). It is not a sandbox.

## License

MIT
