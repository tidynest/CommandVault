# Requirements

Converted from `Project_Plan.odt` and `MoSCoW_Requirements.odt` on 2026-09-10. The plan document was a truncated stub, only its first two sections survived and they are reproduced here. Items marked deferred were moved out of "Could have" the same day so v1 scope stays small. Move them back when wanted.

## Vision

A fast, secure, cross-platform application for storing, categorising and retrieving Linux commands and scripts with a GUI and full-text search. Rust for speed and safety, a modern UI, aimed at developers and security professionals.

## Functional requirements

1. Command storage and retrieval
   - Store commands with descriptions and usage examples
   - Fast full-text search
   - Copy a command to the clipboard with one click
2. Categorisation
   - Hierarchical category structure
   - Multi-tagging for cross-categorisation
   - Custom category creation and management
3. User interface
   - Clean GUI with dark and light modes
   - Keyboard-focused navigation for power users
   - Customisable layouts and views

## MoSCoW

### Must have

- Core functionality with proper error handling
- GUI built on iced
- Modular code organisation
- Basic logging
- Unit tests for critical components

### Should have

- Configuration via file and environment variables
- Comprehensive error messages
- Documentation, inline and README
- Performance optimisations for common operations
- Integration tests

### Could have

- Benchmarking tests
- More export formats. Markdown exists.

### Won't have, deferred on 2026-09-10

- Advanced CLI features, autocomplete and interactive mode
- Telemetry and metrics
- Plugin or extension system
- Legacy system support
- Automatic updates
- Cloud integration

## As built, 0.5.0

Commands with a title, a multi-line command, a multi-line description, tags and a category; the title is optional and falls back to the command's first line. Search needs every term in some field, quoted phrases stay whole, `tag:` and `cat:` prefixes pin a term to one field. Nested categories with subtree filtering and counts, add, rename, move and delete, all undoable. Tags as plain strings with a colour registry, shown as chips in rows and in a sidebar list with counts; click to search, right-click to recolour, double-click to rename, merge or remove. Pin commands to the top. `{{name}}` and `{{name=default}}` placeholders filled in before a copy, values remembered for the session. A copy is wiped from the clipboard after 30 seconds if it is still there. Copy counts and a most-copied order beside title and recency. Twenty undo steps across delete, edit, import, tag and category actions.

One JSON file under the user config directory, saved atomically with a `.bak` of the previous version and a dated copy when another program changed the file in between. Markdown and shell script export of the visible list. Import of frequent commands from zsh, bash or fish history. Reload from disk on Ctrl+R.

Settings in `config.json`: sort order, colour scheme, window size, zoom, the selected category, the sidebar switch and the clipboard wipe delay, the first overridable with `COMMANDVAULT_SORT`. Light and dark follow the desktop unless one of iced's built-in themes is picked. Keyboard covers the whole flow: search, arrows, Home and End, page keys, Enter to copy, Tab between fields, Ctrl shortcuts for new, edit, duplicate, delete, pin, undo, reload, focus and zoom, F2 for a category. Tooltips explain the buttons, the chips and each title's dates. `--version` and `--help` on the command line. F1 lists every key inside the app. Dated backups are capped at ten. Layout customisation is one switch. Ctrl+B hides the sidebar and the choice is saved, nothing else moves.
