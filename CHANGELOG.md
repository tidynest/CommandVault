# Changelog

Versions are tagged on `main`. Dates are the day the version was cut.

## Unreleased

### Files

- Export also writes `export.csv`, one row per command, with formula-looking cells guarded by a leading `'`, and `export.json`, a vault file of the visible commands with their categories and tag colours.

### Development

- Benchmarks in `src/app/tests/bench.rs` time search, sorting, save and load, export and the category tree over ten thousand commands. The README says how to run them.

### Window and keyboard

- Drag the gap on either side of the list to resize the sidebar or the form. Double-click a gap for the default widths. Both are saved, and the list keeps at least 340 pixels.

## 0.5.0, 2026-09-12

### Window and keyboard

- Every built-in iced theme in the colour scheme list, twenty-two of them beside System. `config.json` stores the name.
- Compact rows on Ctrl+Shift+B, title and command only. Saved.

### Files

- A vault another program wrote is reloaded on its own within three seconds.
- A desktop entry and an icon under `assets/`, the README says where they install.

### Commands and search

- The title tooltip says when the command was last copied.
- Search timed at 1.8 ms over ten thousand commands, an ignored test keeps the number honest.

## 0.4.0, 2026-09-12

### Commands and search

- `tag:name` and `cat:name` search prefixes match a whole tag name or a category on the path. A chip click searches with the tag prefix.
- The list scrolls the least distance that shows the highlighted row, measured, so a long command above it no longer hides it.
- A repeat copy of the same text keeps its full clipboard delay.
- The title tooltip says how long ago the command was updated.

### Categories and tags

- Hovered sidebar entries are tinted again.
- The tag list scrolls on its own below the tree instead of being pushed out by it.

### Window and keyboard

- The key panel is modal for the keyboard too. Only Escape and F1 reach it.

### Files

- Export also writes `export.sh`, each command under a comment with its title, description, category and tags.

## 0.3.0, 2026-09-11

The same day as 0.2.0, an evening of small features.

### Commands and search

- A recently copied order. Every copy stamps the command, never-copied ones sort last.
- The clipboard is wiped thirty seconds after a copy, fill or export if it still holds that text. `clear_after` in `config.json` sets the delay, 0 keeps the copy.

### Categories

- Alt+Up and Alt+Down walk the sidebar, All included.
- Double-click a category to rename or move it, like F2.

### Window and keyboard

- Ctrl+B or the button beside Import hides the sidebar and the list takes its width. Saved.
- F1 or the Keys button shows every shortcut over the page. F1, Escape or a click outside closes it.

### Files

- Dated backups, `vault.json.<seconds>.bak`, are capped at the newest ten.

## 0.2.0, 2026-09-11

Everything since the first day. Grouped by area, not by commit.

### Commands and search

- Multi-line command and description fields. Enter submits, Shift+Enter adds a line.
- Optional title, filled from the command's first line.
- Every search term must appear in some field, a quoted phrase stays together.
- `{{name}}` and `{{name=default}}` placeholders, filled in a row before the copy, values remembered for the session, an As is copy for the template.
- Pinned commands sort first under every order. Copy counts, a most-copied order, a copy on double-click, the copied row highlighted.
- Duplicate a command into the form with Ctrl+Shift+E.

### Categories and tags

- Command counts per category, the selected category remembered between runs and used as the default for new commands and subcategories.
- Tags listed in the sidebar with counts. Rename, merge, recolour or remove a tag from its chip. Tooltips on chips.
- F2 renames the selected category.

### Undo

- Twenty steps: delete, edit, import, tag rename and removal, category delete, rename and move. A tag merge is the one action without undo.

### Files

- Reload from disk with Ctrl+R.
- A save that finds the file changed outside the app keeps that version as `vault.json.<epoch>.bak`.
- Import from fish history, and bash timestamp lines skipped.

### Window and keyboard

- Colour scheme setting, window size remembered, zoom with Ctrl+Up and Ctrl+Down with the minimum size scaled along.
- Tab and Shift+Tab between fields, Home, End, PageUp and PageDown in the list, Ctrl+N, Ctrl+E, Ctrl+D, Ctrl+P, Ctrl+F, Ctrl+Z.
- Rows hold together at the 960x600 minimum.
- Tooltips on Export, Import and each title, the latter with created and updated times and the copy count.
- `--version` and `--help`, exit code 2 on an unknown argument.

### Fixed

- Enter in the search box no longer submits the form: an unfocused editor's key binding now ignores keys.
- Ctrl with plus, minus or zero typed into a focused field instead of zooming, hence the arrow keys.
- Undo of an edit keeps a pin or copies made after it.
- Reload drops a selected category the file no longer has.

## 0.1.0, 2026-09-10

First version: list, search, add, edit, copy, delete, nested categories, tag chips with colours, Markdown export, shell history import, sort order in `config.json`, JSON vault saved atomically with a `.bak`, undo for a delete.
