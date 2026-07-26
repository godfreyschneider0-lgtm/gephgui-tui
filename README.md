# geph-tui

A lightweight terminal UI client for the [Geph 5](https://geph.io/) proxy service. Provides SOCKS5 and HTTP proxy with an interactive TUI for configuration, node selection, and connection management.

Powered by [geph-lite](https://github.com/godfreyschneider0-lgtm/geph-lite) — a smol-runtime fork of the geph5 engine that idles at **40–60 MB RSS**.

## Architecture

```
┌─────────────────────────────┐
│  geph-tui process (smol)     │
│  - ratatui rendering          │
│  - keyboard events (crossterm)│
│  - TCP RPC client (nanorpc)   │
└──────────┬──────────────────┘
           │ TCP 127.0.0.1:12222
┌──────────▼──────────────────┐
│  geph5-client subprocess     │
│  - smol single-threaded       │
│  - sosistab3 / picomux        │
│  - SOCKS5 / HTTP proxy        │
└─────────────────────────────┘
```

The TUI is a **thin shell** — it does not link the engine as a library. It drives a separate `geph5-client` subprocess over TCP RPC. Communication uses `nanorpc` (line-delimited JSON-RPC).

## Requirements

- Rust toolchain (latest stable)
- Supported platforms: Linux, macOS, Windows, Android (via Termux)

## Compilation

```sh
git clone --recursive https://github.com/godfreyschneider0-lgtm/geph5-tui.git
./package.sh
```

The build produces two binaries in `target/release/`:
- `geph-tui` — the TUI application
- `geph5-client` — the proxy engine subprocess

## Running

```sh
cargo run --release                  # interactive TUI (default)
geph-tui --ctl start                 # headless daemon start
geph-tui --ctl stop                  # stop daemon
geph-tui --ctl status                # check daemon status
geph-tui --ctl switch                # hot-swap to a new exit node
geph-tui --ctl switch jp --immediate # switch to Japan immediately
geph-tui --ctl exit                  # show current exit constraint
geph-tui --ctl exits                 # list available exit nodes
geph-tui --ctl sessions              # show active + draining sessions
geph-tui --ctl logs 50               # last 50 log lines (default 20)
geph-tui --ctl account               # show account info
geph-tui --ctl redeem ABC123         # redeem a voucher code
```

Or use the bundled `gephctl` script:

```sh
gephctl start    # start the daemon
gephctl stop     # stop the daemon
gephctl status   # show connection status
gephctl log      # tail the log
gephctl restart  # stop then start
```

Environment variables:
```
  GEPH_BIN          geph-tui binary path (default: alongside gephctl)
  GEPH_LOG          log file path (default: $TMPDIR/geph/geph-tui.log or /tmp/geph/geph-tui.log)
  GEPH_SOCKS_PORT   SOCKS5 port (default: 9909)
  GEPH_HTTP_PORT    HTTP proxy port (default: 9910)
```

Default ports: SOCKS5 on `9909`, HTTP proxy on `9910`.

### Keybindings (TUI)

**Global keys:**

| Key | Action |
|-----|--------|
| `1`–`5` | Switch tabs (Status / Nodes / Config / Debug / Plus) |
| `s` / `x` | Start / stop connection |
| `q` | Quit |

**Nodes tab:**

| Key | Action |
|-----|--------|
| `Up`/`Down` + `Enter` | Select exit region |
| `a` | Switch to auto (deselect country) |

**Config tab:**

| Key | Action |
|-----|--------|
| `e` | Edit Account ID |
| `p` / `h` | Edit SOCKS5 port / HTTP port |
| `l` | Toggle listen-all-interfaces |
| `b` | Toggle direct vs bridged mode |
| `r` | Register a new account |

**Status tab:**

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down / up |

**Debug tab:**

| Key | Action |
|-----|--------|
| `Up`/`Down` | Scroll logs |
| `d` | Toggle debug log capture |

**Plus tab:**

| Key | Action |
|-----|--------|
| `v` | Edit redeem code |
| `o` | Edit promo code |
| `c` | Clear URL |
| `Up`/`Down` | Select price tier |
| `Left`/`Right` | Select payment method |
| `Enter` | Redeem voucher |
| `b` | Buy subscription |

**Focus mode (when editing a text field):**

| Key | Action |
|-----|--------|
| `Esc` / `Enter` | Exit field edit |

## Persisted files

- **Preferences**: `{config_dir}/geph5_tui_prefs.json` — TUI settings (account, ports, etc.)
  - Linux: `~/.config/geph5_tui_prefs.json`
  - macOS: `~/Library/Application Support/geph5_tui_prefs.json`
  - Windows: `%APPDATA%\geph5_tui_prefs.json`
- **Debug log**: `gephgui.log` — written to the current working directory when debug logging is enabled
- **Connection cache**: `{cache_dir}/geph5_tui/database.db` (+ `-wal`, `-shm` SQLite WAL files)
  - Linux: `~/.cache/geph5_tui/`
  - macOS: `~/Library/Caches/geph5_tui/`
  - Windows: `%LOCALAPPDATA%\geph5_tui\`
- **Update cache**: `{cache_dir}/geph5-dl/` — downloaded update archives + metadata

## Self-update

geph-tui periodically checks for updates in the background (mean interval ~6 hours, Poisson-sampled). Updates are downloaded and cached; on next startup the user is prompted to apply. Cache is stored in `{cache_dir}/geph5-dl/`. Tracks: `linux-stable`, `windows-stable`, `macos-stable`, `android` (android uses linux-stable track).

## Packaging

### Linux (.deb)

```sh
./package.sh                  # cargo-deb (native amd64)
./package.sh --arm64          # cross-compile for aarch64
./package.sh --manual         # manual dpkg-deb fallback
./package.sh --install        # build + install immediately
./package.sh --skip-build     # cargo deb --no-build (use existing binary)
```

Install the produced `.deb`:
```sh
sudo dpkg -i geph-tui_*.deb
sudo apt remove geph-tui      # remove
```

### Windows

```sh
# Prerequisites: rustup target add x86_64-pc-windows-gnu; sudo apt install mingw-w64
./package-windows.sh                  # cross-compile + produce scoop-ready zip
./package-windows.sh --skip-build     # zip from existing binaries
```

The zip contains `geph-tui.exe` and `geph5-client.exe`. A scoop manifest is at `package/scoop/geph-tui.json`.

### Termux (Android)

```sh
git clone https://github.com/termux/termux-packages.git
cp -r packages/geph-tui termux-packages/packages/
cd termux-packages
TERMUX_DOCKER_RUN_EXTRA_ARGS="--security-opt apparmor=unconfined" \
    ./scripts/run-docker.sh ./build-package.sh -I -f geph-tui
```

## Development

Testing:
```sh
./scripts/test-registration.sh                    # end-to-end daemon + registration smoke test
./scripts/test-registration.sh path/to/geph5-client # test a specific binary
```

The test verifies: AWS Lambda transport compiled in, daemon survives empty-secret startup, registration RPC works, registration makes progress.

## Project structure

```
geph-tui/
├── src/                  # TUI application
│   ├── main.rs           # entry point, CLI args, event loop
│   ├── state.rs          # AppState, TuiPrefs, persisted config
│   ├── daemon.rs         # subprocess lifecycle + TCP RPC transport
│   ├── event.rs          # keyboard handling
│   ├── autoupdate.rs     # background self-update download loop
│   ├── ui/               # ratatui rendering
│   │   ├── mod.rs        # tab layout + dispatch
│   │   ├── status.rs     # connection status tab
│   │   ├── nodes.rs      # exit node selection tab
│   │   ├── config.rs     # settings tab
│   │   ├── debug.rs      # log viewer tab
│   │   └── plus.rs       # subscription/voucher tab
│   └── default-config.yaml
├── geph-lite/            # geph-lite engine (git submodule)
├── build.rs              # Windows resource embedding (icon)
├── Cargo.toml            # workspace + package manifest
├── package.sh            # Linux .deb packaging
├── package-windows.sh    # Windows cross-compile + scoop zip
├── gephctl               # daemon control script (bash)
├── scripts/
│   └── test-registration.sh  # end-to-end registration smoke test
├── packages/geph-tui/    # termux package definition
└── package/              # debian + scoop templates
    ├── DEBIAN/           # control.template, postinst, postrm
    ├── deb-copyright
    ├── deb-scripts/
    └── scoop/
        └── geph-tui.json # scoop manifest
```

## License

MPL 2.0.
