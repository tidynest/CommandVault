# Project structure

Converted from `Project_Structure.odt` on 2026-09-10.

## As built

```
CommandVault/
├── .cargo/audit.toml     # cargo-audit ignore list with the reason per advisory
├── .github/workflows/ci.yml   # fmt, clippy, test, build, cargo-deny
├── .gitlab-ci.yml
├── CHANGELOG.md
├── Cargo.toml
├── LICENSE               # MIT
├── README.md
├── deny.toml             # cargo-deny licence allow list and advisory ignores
├── src/
│   ├── main.rs           # --version and --help, settings load, window setup, entry point
│   ├── app.rs            # App state, Message, update, helpers
│   ├── app/tests.rs      # Message-loop tests
│   ├── view.rs           # view, view_sidebar, widget helpers, two pure-helper tests
│   ├── config.rs         # Settings, Sort, Scheme, config.json, env override
│   ├── export.rs         # Markdown export
│   ├── history.rs        # zsh, bash and fish history import
│   ├── model.rs          # Command, Category, Tag, Draft, Vault, search, placeholders, tree
│   └── storage.rs        # Vault path, load, atomic save, conflict copy
├── tests/cli.rs          # Runs the built binary with --version, --help and a bad flag
├── assets/               # Desktop entry and SVG icon for launchers
├── docs/                 # This file, REQUIREMENTS.md, screenshot.png, Flowchart.odg
├── images/               # Design diagrams, SVG
└── image_data/           # Lucidchart CSV exports behind the SVGs
```

Data lives at `$XDG_CONFIG_HOME/commandvault/vault.json`, falling back to `~/.config/commandvault/vault.json`, with `config.json`, `export.md` and the `.bak` copies beside it.

## Planned growth

The original template proposed `gui/`, `model/`, `services/` and `utils/` directories. None became necessary: `model.rs` holds the Tag registry and the placeholder parser at about 600 lines, `view.rs` is one screen, and there is no business logic beyond the vault. Split a file when a second screen or a second storage format appears, not before. `tests/` exists, with the one integration test.
