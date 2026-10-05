use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use iced::advanced::widget::operation::{Outcome, Scrollable};
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key};
use iced::widget::scrollable::AbsoluteOffset;
use iced::widget::text_editor;
use iced::{Event, Rectangle, Size, Subscription, Task, Vector, event, mouse, window};
use uuid::Uuid;

use crate::config::{self, Scheme, Settings, Sort, WindowSize};
use crate::export;
use crate::history;
use crate::model::{Category, Command, Draft, Tag, Vault, fill, placeholders, title_for};
use crate::storage;

pub(crate) const SEARCH_ID: &str = "search";
pub(crate) const TITLE_ID: &str = "title";
pub(crate) const TAG_RENAME_ID: &str = "tag-rename";
pub(crate) const FILL_ID: &str = "fill";
pub(crate) const CATEGORY_ID: &str = "category";
pub(crate) const LIST_ID: &str = "list";
/// How often the vault file is checked for a write by another program.
const POLL: Duration = Duration::from_secs(3);

/// Three columns need this much before they start to overlap, at zoom 1.0.
pub(crate) const MIN_SIZE: Size = Size::new(960.0, 600.0);
/// Rows a PageUp or PageDown jumps. Rows have no fixed height to measure a page by.
const PAGE: isize = 10;
/// Undo steps kept. Each holds a whole command or a list of ids, so this stays small.
const UNDO_DEPTH: usize = 20;
const ZOOM_STEP: f32 = 0.1;
const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.0;
/// Page padding on each side, and the width of a draggable gap between two columns.
pub(crate) const EDGE: f32 = 16.0;
pub(crate) const GAP: f32 = 24.0;
/// The narrowest each column may get. The default widths and `LIST_MIN` fill a window
/// at `MIN_SIZE` exactly.
const SIDEBAR_MIN: f32 = 160.0;
const FORM_MIN: f32 = 260.0;
const LIST_MIN: f32 = 340.0;

/// The gap being dragged: left of the list resizes the sidebar, right of it the form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Divider {
    Sidebar,
    Form,
}

pub(crate) struct App {
    /// `None` when there is nowhere safe to write, so edits stay in memory only.
    pub(crate) path: Option<PathBuf>,
    /// The vault file's modification time after the last load or save. A different
    /// time at the next save means another program wrote the file in between.
    pub(crate) vault_mtime: Option<SystemTime>,
    pub(crate) vault: Vault,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) settings: Settings,
    pub(crate) search: String,
    /// Highlighted row. Up and Down move it, Enter copies it.
    pub(crate) selected: Option<Uuid>,
    /// Sidebar filter. `None` shows every command.
    pub(crate) selected_category: Option<Uuid>,
    pub(crate) new_category: String,
    pub(crate) new_category_parent: Option<Uuid>,
    /// Category being renamed or moved through the sidebar form.
    pub(crate) editing_category: Option<Uuid>,
    pub(crate) form: Form,
    pub(crate) status: String,
    /// Steps back, newest last. Delete keeps the command, Import keeps the ids it added.
    pub(crate) undo: Vec<Undo>,
    /// Inline rename in the status row, started by double-clicking a chip.
    pub(crate) tag_rename: Option<TagRename>,
    /// Values for a command's `{{placeholders}}`, asked for in the status row before the copy.
    pub(crate) fill: Option<Fill>,
    /// The last value typed for each placeholder name, offered again this session.
    pub(crate) fill_memory: HashMap<String, String>,
    /// The key panel over the page, F1 or the Keys button.
    pub(crate) help: bool,
    /// Bumped by every copy. A clipboard timer carries the value it started with and
    /// wipes nothing once a later copy has moved it on.
    pub(crate) copy_generation: u64,
    /// The sidebar entry under the pointer, `Some(None)` for All. View only.
    pub(crate) hovered: Option<Option<Uuid>>,
    /// The gap under a held mouse button, with the column's width when the press began.
    pub(crate) dragging: Option<(Divider, f32)>,
}

pub(crate) struct Fill {
    pub(crate) id: Uuid,
    pub(crate) names: Vec<String>,
    pub(crate) values: Vec<String>,
}

pub(crate) enum Undo {
    Deleted(Command),
    /// The command as it was before an edit was saved.
    Edited(Command),
    Imported(Vec<Uuid>),
    /// A deleted category with the ids of the categories and commands it held.
    CategoryRemoved {
        category: Category,
        children: Vec<Uuid>,
        commands: Vec<Uuid>,
    },
    /// A tag taken off every command, with the ids that carried it.
    TagRemoved {
        tag: Tag,
        ids: Vec<Uuid>,
    },
    /// A category's name and parent before a rename or move.
    CategoryChanged {
        id: Uuid,
        name: String,
        parent_id: Option<Uuid>,
    },
    /// A tag renamed onto a fresh name. A merge onto an existing tag is not recorded.
    TagRenamed {
        from: String,
        to: String,
    },
}

pub(crate) struct TagRename {
    pub(crate) old: String,
    pub(crate) text: String,
}

/// Raw form text, plus the two editors' own state, which can hold several lines.
/// `editing` holds the id of the command being edited, if any.
#[derive(Default)]
pub(crate) struct Form {
    pub(crate) editing: Option<Uuid>,
    pub(crate) title: String,
    pub(crate) description: text_editor::Content,
    pub(crate) command: text_editor::Content,
    pub(crate) tags: String,
    pub(crate) category_id: Option<Uuid>,
}

impl Form {
    fn from_command(c: &Command) -> Self {
        Self {
            editing: Some(c.id),
            title: c.title.clone(),
            description: text_editor::Content::with_text(&c.description),
            command: text_editor::Content::with_text(&c.command_text),
            tags: c.tags.join(", "),
            category_id: c.category_id,
        }
    }

    /// Trimmed and parsed. An empty title becomes the command's first line.
    fn draft(&self) -> Draft {
        let command_text: String = self.command.text().trim().into();
        let title = match self.title.trim() {
            "" => title_for(&command_text),
            t => t.into(),
        };
        Draft {
            title,
            description: self.description.text().trim().into(),
            command_text,
            tags: self
                .tags
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(String::from)
                .collect(),
            category_id: self.category_id,
        }
    }
}

/// A category as shown in a pick list. `id` of `None` is the "no category" entry.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Choice {
    pub(crate) id: Option<Uuid>,
    pub(crate) label: String,
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    SearchChanged(String),
    TitleChanged(String),
    DescriptionAction(text_editor::Action),
    /// Typing, cursor moves and pastes in the command editor.
    CommandAction(text_editor::Action),
    TagsChanged(String),
    FormCategory(Choice),
    Submit,
    /// Loads a command into the form and focuses the title.
    Edit(Uuid),
    /// Ctrl+N: an empty form with the title focused.
    New,
    /// Ctrl+E: edit the highlighted row.
    EditSelected,
    /// Ctrl+Shift+E: the highlighted row's fields in the form, as a new command.
    DuplicateSelected,
    /// Ctrl+D, or Delete with nothing focused: delete the highlighted row.
    DeleteSelected,
    /// The star on a row, or Ctrl+P on the highlight: pinned commands sort first.
    TogglePin(Uuid),
    Cancel,
    Copy(Uuid),
    /// Enter copies the selected row, or the first visible match when nothing is selected.
    CopySelected,
    Select(Uuid),
    /// Moves the selection by this many rows, clamped to the visible list.
    Move(isize),
    /// Alt+Up and Alt+Down: the sidebar selection through All and the tree, clamped.
    CategoryStep(isize),
    Key(keyboard::Event),
    /// Escape with nothing focused: clear the search and highlight, focus the search box.
    Reset,
    /// Ctrl+F: focus the search box and change nothing else.
    FocusSearch,
    /// Tab and Shift+Tab: the next or previous text field in tree order.
    FocusNext,
    FocusPrevious,
    SortChanged(Sort),
    SchemeChanged(Scheme),
    /// Kept in settings as it happens, written to disk when the window closes.
    Resized(Size),
    /// A new scale factor for the whole UI, clamped to `ZOOM_RANGE` and saved.
    Zoom(f32),
    /// Ctrl+Up and Ctrl+Down: so many steps of `ZOOM_STEP` from the current zoom.
    ZoomStep(i8),
    /// Ctrl+B or the header button: show or hide the sidebar, saved.
    ToggleSidebar,
    /// Ctrl+Shift+B: rows with or without description, path and chips, saved.
    ToggleCompact,
    /// Every few seconds from `ticks`: reload if another program wrote the vault.
    Tick,
    /// F1, the Keys button or a click on the shade: show or hide the key panel.
    ToggleHelp,
    /// Escape captured by a focused widget: closes the key panel, nothing else.
    CloseHelp,
    /// The pointer entered a sidebar entry, or left one with `None`.
    HoverCategory(Option<Option<Uuid>>),
    /// A press on a gap beside the list. Mouse events reach `update` until the release.
    DragStart(Divider),
    /// The pointer's x while a gap is held.
    DragTo(f32),
    /// Button released or pointer gone: saves the widths if they changed.
    DragEnd,
    /// A double-click on a gap: both columns back to their default widths, saved.
    ResetWidths,
    /// The clipboard read back `Settings.clear_after` seconds after `copied` went in.
    /// Wiped only while it still holds that text and no later copy has been made.
    ClipboardRead {
        copied: String,
        now: Option<String>,
        generation: u64,
    },
    /// The window's close button. Saves settings, then closes, which ends the app.
    CloseRequested(window::Id),
    /// Markdown of the visible commands to `export.md` beside the vault and to the
    /// clipboard, and a shell script of the same commands to `export.sh`.
    Export,
    /// Puts the last deleted command back, or removes the last import.
    Undo,
    /// The 50 most used commands from the shell history, used twice or more, into the selected category.
    ImportHistory,
    /// Ctrl+R: read the vault file again, for changes made outside the app.
    Reload,
    /// Right-click on a chip: next palette colour for that tag.
    CycleTagColour(String),
    /// Double-click on a chip: rename it everywhere.
    TagRenameStart(String),
    TagRenameChanged(String),
    TagRenameSubmit,
    TagRenameCancel,
    /// The Remove button in the rename row: the tag comes off every command.
    TagRemove,
    /// A value typed into the placeholder row, by position.
    FillChanged(usize, String),
    /// Copies the command with its placeholders filled in.
    FillSubmit,
    /// Copies the command with its markers as written.
    FillRaw,
    FillCancel,
    Delete(Uuid),
    SelectCategory(Option<Uuid>),
    NewCategoryChanged(String),
    NewCategoryParent(Choice),
    SubmitCategory,
    /// Loads the selected category into the sidebar form for renaming or moving, and
    /// focuses the name. Also F2.
    EditCategory,
    CancelCategory,
    /// Removes the category selected in the sidebar.
    DeleteCategory,
}

impl App {
    /// `settings` come from `main`, which needs them before the window opens.
    pub(crate) fn new(settings: Settings) -> (Self, Task<Message>) {
        let mut path = storage::vault_path();
        let config_path = path.as_deref().map(config::config_path);
        let (vault, status) = match path.as_deref().map(storage::load::<Vault>) {
            Some(Ok(mut vault)) => {
                vault.ensure_tags();
                log::info!("loaded {} commands", vault.commands.len());
                let status = format!(
                    "Loaded {} commands from {}.",
                    vault.commands.len(),
                    path.as_deref().map_or_else(String::new, storage::tidy)
                );
                (vault, status)
            }
            Some(Err(e)) => {
                // Never overwrite a vault we could not read.
                log::warn!("could not read vault, saving disabled: {e}");
                path = None;
                (
                    Vault::default(),
                    format!("Could not read vault: {e}. Saving disabled."),
                )
            }
            None => {
                log::warn!("no config directory found, saving disabled");
                (
                    Vault::default(),
                    "No config directory found. Saving disabled.".into(),
                )
            }
        };
        // A category deleted from a hand-edited file must not linger as a filter.
        let category = settings
            .category
            .filter(|id| vault.categories.iter().any(|c| c.id == *id));
        let vault_mtime = path.as_deref().and_then(storage::modified);
        let app = Self {
            path,
            vault_mtime,
            vault,
            config_path,
            settings,
            search: String::new(),
            selected: None,
            selected_category: category,
            new_category: String::new(),
            new_category_parent: category,
            editing_category: None,
            form: Form {
                category_id: category,
                ..Form::default()
            },
            status,
            undo: Vec::new(),
            tag_rename: None,
            fill: None,
            help: false,
            copy_generation: 0,
            hovered: None,
            dragging: None,
            fill_memory: HashMap::new(),
        };
        (app, iced::widget::operation::focus(SEARCH_ID))
    }

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SearchChanged(s) => self.search = s,
            Message::TitleChanged(s) => self.form.title = s,
            Message::DescriptionAction(a) => self.form.description.perform(a),
            Message::CommandAction(a) => self.form.command.perform(a),
            Message::TagsChanged(s) => self.form.tags = s,
            Message::FormCategory(c) => self.form.category_id = c.id,
            Message::Submit => {
                let draft = self.form.draft();
                if !draft.is_valid() {
                    self.status = "A command is required.".into();
                    return Task::none();
                }
                let duplicate = self.duplicate_of(&draft);
                match self
                    .form
                    .editing
                    .and_then(|id| self.vault.commands.iter_mut().find(|c| c.id == id))
                {
                    Some(existing) => {
                        let before = existing.clone();
                        existing.apply(draft);
                        self.remember(Undo::Edited(before));
                    }
                    None => self.vault.commands.push(Command::new(draft)),
                }
                self.vault.ensure_tags();
                self.reset_form();
                if self.persist()
                    && let Some(title) = duplicate
                {
                    self.status = format!("Saved. Same command text as \"{title}\".");
                }
            }
            Message::Edit(id) => {
                if let Some(c) = self.vault.commands.iter().find(|c| c.id == id) {
                    self.form = Form::from_command(c);
                    self.selected = Some(id);
                    return iced::widget::operation::focus(TITLE_ID);
                }
            }
            Message::New => {
                self.reset_form();
                return iced::widget::operation::focus(TITLE_ID);
            }
            Message::EditSelected => {
                if let Some(id) = self.selected_visible() {
                    return self.update(Message::Edit(id));
                }
            }
            Message::DuplicateSelected => {
                let source = self
                    .selected_visible()
                    .and_then(|id| self.vault.commands.iter().find(|c| c.id == id));
                if let Some(c) = source {
                    self.form = Form {
                        editing: None,
                        ..Form::from_command(c)
                    };
                    return iced::widget::operation::focus(TITLE_ID);
                }
            }
            Message::DeleteSelected => {
                if let Some(id) = self.selected_visible() {
                    return self.update(Message::Delete(id));
                }
            }
            Message::TogglePin(id) => {
                if let Some(c) = self.vault.commands.iter_mut().find(|c| c.id == id) {
                    c.pinned = !c.pinned;
                    let (title, pinned) = (c.title.clone(), c.pinned);
                    if self.persist() {
                        self.status = if pinned {
                            format!("Pinned \"{title}\" to the top.")
                        } else {
                            format!("Unpinned \"{title}\".")
                        };
                    }
                }
            }
            Message::Cancel => self.reset_form(),
            Message::Delete(id) => {
                let Some(pos) = self.vault.commands.iter().position(|c| c.id == id) else {
                    return Task::none();
                };
                let removed = self.vault.commands.remove(pos);
                if self.selected == Some(id) {
                    self.selected = None;
                }
                if self.form.editing == Some(id) {
                    self.reset_form();
                }
                if self.fill.as_ref().is_some_and(|f| f.id == id) {
                    self.fill = None;
                }
                let title = removed.title.clone();
                self.remember(Undo::Deleted(removed));
                if self.persist() {
                    self.status = format!("Deleted \"{title}\". Undo or Ctrl+Z restores it.");
                }
            }
            Message::CycleTagColour(name) => {
                if self.vault.cycle_tag_colour(&name) && self.persist() {
                    self.status =
                        format!("Tag \"{name}\" recoloured. Right-click again for the next one.");
                }
            }
            Message::TagRenameStart(old) => {
                self.tag_rename = Some(TagRename {
                    text: old.clone(),
                    old,
                });
                return iced::widget::operation::focus(TAG_RENAME_ID);
            }
            Message::TagRenameChanged(text) => {
                if let Some(r) = &mut self.tag_rename {
                    r.text = text;
                }
            }
            Message::TagRenameSubmit => {
                let Some(r) = self.tag_rename.take() else {
                    return Task::none();
                };
                let new = r.text.trim().to_owned();
                let merged = self.vault.tag_colour(&new).is_some();
                if self.vault.rename_tag(&r.old, &new) {
                    if !merged {
                        self.remember(Undo::TagRenamed {
                            from: r.old.clone(),
                            to: new.clone(),
                        });
                    }
                    if self.persist() {
                        self.status = if merged {
                            format!(
                                "Merged tag \"{}\" into \"{new}\". No undo for a merge.",
                                r.old
                            )
                        } else {
                            format!("Renamed tag \"{}\" to \"{new}\".", r.old)
                        };
                    }
                } else {
                    self.status =
                        "Tag name is required, must differ and cannot contain a comma.".into();
                }
                return iced::widget::operation::focus(SEARCH_ID);
            }
            Message::TagRenameCancel => self.tag_rename = None,
            Message::TagRemove => {
                let Some(r) = self.tag_rename.take() else {
                    return Task::none();
                };
                if let Some((tag, ids)) = self.vault.remove_tag(&r.old) {
                    let (name, count) = (tag.name.clone(), ids.len());
                    self.remember(Undo::TagRemoved { tag, ids });
                    if self.persist() {
                        self.status = format!(
                            "Removed tag \"{name}\" from {count} commands. Undo puts it back."
                        );
                    }
                }
                return iced::widget::operation::focus(SEARCH_ID);
            }
            Message::ImportHistory => {
                let Some(path) = history::history_path() else {
                    self.status =
                        "No shell history file found. Set HISTFILE to point at one.".into();
                    return Task::none();
                };
                match history::read(&path) {
                    Err(e) => self.status = format!("Could not read {}: {e}", path.display()),
                    Ok(text) => self.import_history(&text, &storage::tidy(&path)),
                }
            }
            Message::Reload => {
                let Some(path) = &self.path else {
                    self.status = "No vault file to reload, saving is disabled.".into();
                    return Task::none();
                };
                match storage::load::<Vault>(path) {
                    Ok(mut vault) => {
                        vault.ensure_tags();
                        self.vault = vault;
                        self.vault_mtime = storage::modified(path);
                        self.status = format!("Reloaded {} commands.", self.vault.commands.len());
                        if let Some(id) = self.selected_category
                            && !self.vault.categories.iter().any(|c| c.id == id)
                        {
                            return self.update(Message::SelectCategory(None));
                        }
                    }
                    Err(e) => {
                        self.status = format!("Could not reload: {e}. Keeping what is loaded.")
                    }
                }
            }
            Message::Undo => match self.undo.pop() {
                Some(Undo::Deleted(c)) => {
                    let title = c.title.clone();
                    self.vault.commands.push(c);
                    self.vault.ensure_tags();
                    if self.persist() {
                        self.status = format!("Restored \"{title}\".");
                    }
                }
                Some(Undo::Edited(mut before)) => {
                    let title = before.title.clone();
                    if let Some(c) = self.vault.commands.iter_mut().find(|c| c.id == before.id) {
                        // Pins and copies made after the edit are not part of it.
                        before.pinned = c.pinned;
                        before.copies = c.copies;
                        *c = before;
                    } else {
                        self.vault.commands.push(before);
                    }
                    self.vault.ensure_tags();
                    if self.persist() {
                        self.status = format!("Restored the previous \"{title}\".");
                    }
                }
                Some(Undo::CategoryRemoved {
                    category,
                    children,
                    commands,
                }) => {
                    let (id, name) = (category.id, category.name.clone());
                    self.vault.categories.push(category);
                    for c in &mut self.vault.categories {
                        if children.contains(&c.id) {
                            c.parent_id = Some(id);
                        }
                    }
                    for c in &mut self.vault.commands {
                        if commands.contains(&c.id) {
                            c.category_id = Some(id);
                        }
                    }
                    if self.persist() {
                        self.status = format!("Restored category \"{name}\".");
                    }
                }
                Some(Undo::CategoryChanged {
                    id,
                    name,
                    parent_id,
                }) => {
                    let shown = name.clone();
                    if self.vault.update_category(id, name, parent_id) && self.persist() {
                        self.status = format!("Category is \"{shown}\" again, where it was.");
                    }
                }
                Some(Undo::TagRenamed { from, to }) => {
                    if self.vault.rename_tag(&to, &from) && self.persist() {
                        self.status = format!("Tag is \"{from}\" again.");
                    }
                }
                Some(Undo::TagRemoved { tag, ids }) => {
                    let name = tag.name.clone();
                    for c in &mut self.vault.commands {
                        if ids.contains(&c.id) && !c.tags.contains(&name) {
                            c.tags.push(name.clone());
                        }
                    }
                    if !self.vault.tags.iter().any(|t| t.name == name) {
                        self.vault.tags.push(tag);
                    }
                    if self.persist() {
                        self.status = format!("Restored tag \"{name}\".");
                    }
                }
                Some(Undo::Imported(ids)) => {
                    self.vault.commands.retain(|c| !ids.contains(&c.id));
                    if self.selected.is_some_and(|s| ids.contains(&s)) {
                        self.selected = None;
                    }
                    if self.form.editing.is_some_and(|e| ids.contains(&e)) {
                        self.reset_form();
                    }
                    if self.persist() {
                        self.status = format!("Removed the {} imported commands.", ids.len());
                    }
                }
                None => {}
            },
            Message::Copy(id) => {
                let Some(c) = self.vault.commands.iter().find(|c| c.id == id) else {
                    return Task::none();
                };
                let markers = placeholders(&c.command_text);
                if markers.is_empty() {
                    let text = c.command_text.clone();
                    return self.copy_now(id, text);
                }
                self.selected = Some(id);
                let (names, values) = markers
                    .into_iter()
                    .map(|(name, default)| {
                        let value = self.fill_memory.get(&name).cloned().unwrap_or(default);
                        (name, value)
                    })
                    .unzip();
                self.fill = Some(Fill { id, names, values });
                return iced::widget::operation::focus(FILL_ID);
            }
            Message::FillChanged(i, value) => {
                if let Some(f) = &mut self.fill
                    && let Some(slot) = f.values.get_mut(i)
                {
                    *slot = value;
                }
            }
            Message::FillSubmit => {
                let Some(f) = self.fill.take() else {
                    return Task::none();
                };
                let Some(c) = self.vault.commands.iter().find(|c| c.id == f.id) else {
                    return Task::none();
                };
                let pairs: Vec<(String, String)> = f.names.into_iter().zip(f.values).collect();
                let text = fill(&c.command_text, &pairs);
                for (name, value) in pairs {
                    if !value.is_empty() {
                        self.fill_memory.insert(name, value);
                    }
                }
                return self.copy_now(f.id, text);
            }
            Message::FillRaw => {
                let Some(f) = self.fill.take() else {
                    return Task::none();
                };
                let Some(c) = self.vault.commands.iter().find(|c| c.id == f.id) else {
                    return Task::none();
                };
                let text = c.command_text.clone();
                return self.copy_now(f.id, text);
            }
            Message::FillCancel => self.fill = None,
            Message::CopySelected => {
                let target = self
                    .selected_visible()
                    .or_else(|| self.visible().first().map(|c| c.id));
                if let Some(id) = target {
                    return self.update(Message::Copy(id));
                }
            }
            Message::Select(id) => self.selected = Some(id),
            Message::Move(delta) => {
                let ids: Vec<Uuid> = self.visible().iter().map(|c| c.id).collect();
                self.selected = step(&ids, self.selected, delta);
                if let Some(id) = self.selected {
                    return reveal(id);
                }
            }
            Message::CategoryStep(delta) => {
                let order: Vec<Option<Uuid>> = std::iter::once(None)
                    .chain(self.vault.tree().into_iter().map(|(_, c)| Some(c.id)))
                    .collect();
                let at = order
                    .iter()
                    .position(|c| *c == self.selected_category)
                    .map_or(0, |i| i as isize);
                let next = at.saturating_add(delta).clamp(0, order.len() as isize - 1);
                return self.update(Message::SelectCategory(order[next as usize]));
            }
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                // The key panel is modal. Escape falls through to Reset, which closes
                // it, and F1 never arrives here since on_event maps it first.
                if self.help && !matches!(key.as_ref(), keyboard::Key::Named(key::Named::Escape)) {
                    return Task::none();
                }
                let next = match key.as_ref() {
                    keyboard::Key::Character("z") if modifiers.control() => Message::Undo,
                    // Ctrl with a letter reaches here from a focused input, the control
                    // character it produces is filtered out. Ctrl with "+" or "-" does not,
                    // the input types the plain character, hence arrows for zoom.
                    keyboard::Key::Named(key::Named::ArrowDown) if modifiers.alt() => {
                        Message::CategoryStep(1)
                    }
                    keyboard::Key::Named(key::Named::ArrowUp) if modifiers.alt() => {
                        Message::CategoryStep(-1)
                    }
                    keyboard::Key::Named(key::Named::ArrowUp) if modifiers.control() => {
                        Message::ZoomStep(1)
                    }
                    keyboard::Key::Named(key::Named::ArrowDown) if modifiers.control() => {
                        Message::ZoomStep(-1)
                    }
                    keyboard::Key::Character("n") if modifiers.control() => Message::New,
                    keyboard::Key::Character("b" | "B")
                        if modifiers.control() && modifiers.shift() =>
                    {
                        Message::ToggleCompact
                    }
                    keyboard::Key::Character("b") if modifiers.control() => Message::ToggleSidebar,
                    keyboard::Key::Character("f") if modifiers.control() => Message::FocusSearch,
                    keyboard::Key::Character("r") if modifiers.control() => Message::Reload,
                    keyboard::Key::Character("p") if modifiers.control() => {
                        match self.selected_visible() {
                            Some(id) => Message::TogglePin(id),
                            None => return Task::none(),
                        }
                    }
                    keyboard::Key::Character("e" | "E")
                        if modifiers.control() && modifiers.shift() =>
                    {
                        Message::DuplicateSelected
                    }
                    keyboard::Key::Character("e") if modifiers.control() => Message::EditSelected,
                    keyboard::Key::Character("d") if modifiers.control() => Message::DeleteSelected,
                    keyboard::Key::Named(key::Named::Delete) => Message::DeleteSelected,
                    keyboard::Key::Named(key::Named::ArrowDown) => Message::Move(1),
                    keyboard::Key::Named(key::Named::ArrowUp) => Message::Move(-1),
                    keyboard::Key::Named(key::Named::PageDown) => Message::Move(PAGE),
                    keyboard::Key::Named(key::Named::PageUp) => Message::Move(-PAGE),
                    keyboard::Key::Named(key::Named::End) => Message::Move(isize::MAX),
                    keyboard::Key::Named(key::Named::Home) => Message::Move(isize::MIN),
                    keyboard::Key::Named(key::Named::Enter) => Message::CopySelected,
                    keyboard::Key::Named(key::Named::Escape) => Message::Reset,
                    keyboard::Key::Named(key::Named::F2) => Message::EditCategory,
                    keyboard::Key::Named(key::Named::Tab) if modifiers.shift() => {
                        Message::FocusPrevious
                    }
                    keyboard::Key::Named(key::Named::Tab) => Message::FocusNext,
                    _ => return Task::none(),
                };
                return self.update(next);
            }
            Message::Key(_) => {}
            Message::SortChanged(sort) => {
                self.settings.sort = sort;
                self.save_settings(&format!("Sorting {}", sort.to_string().to_lowercase()));
            }
            Message::SchemeChanged(scheme) => {
                let what = format!("{scheme} colours");
                self.settings.scheme = scheme;
                self.save_settings(&what);
            }
            Message::Resized(size) => {
                // iced reports the size divided by the zoom. The window is reopened
                // through winit, which knows nothing of the zoom, so store the size
                // as winit sees it.
                self.settings.window = Some(WindowSize {
                    width: size.width * self.settings.zoom,
                    height: size.height * self.settings.zoom,
                });
            }
            Message::ZoomStep(steps) => {
                let zoom = ZOOM_STEP.mul_add(f32::from(steps), self.settings.zoom);
                return self.update(Message::Zoom(zoom));
            }
            Message::ClipboardRead {
                copied,
                now,
                generation,
            } => {
                if generation == self.copy_generation && now.as_deref() == Some(copied.as_str()) {
                    self.status = "Clipboard cleared.".into();
                    return iced::clipboard::write(String::new());
                }
            }
            Message::ToggleHelp => self.help = !self.help,
            Message::CloseHelp => self.help = false,
            Message::HoverCategory(entry) => self.hovered = entry,
            Message::DragStart(divider) => self.dragging = Some((divider, self.width(divider))),
            Message::DragTo(x) => {
                let (sidebar, form) = self.widths();
                let room = self.room();
                // The gap's centre follows the pointer, and the list keeps LIST_MIN.
                match self.dragging {
                    Some((Divider::Sidebar, _)) => {
                        self.settings.sidebar_width =
                            (x - EDGE - GAP / 2.0).min(room - form).max(SIDEBAR_MIN);
                    }
                    Some((Divider::Form, _)) => {
                        self.settings.form_width = (self.window_width() - EDGE - GAP / 2.0 - x)
                            .min(room - sidebar)
                            .max(FORM_MIN);
                    }
                    None => {}
                }
            }
            Message::DragEnd => {
                if let Some((divider, before)) = self.dragging.take()
                    && self.width(divider) != before
                {
                    self.save_settings("Column widths saved");
                }
            }
            Message::ResetWidths => {
                self.dragging = None;
                self.settings.sidebar_width = config::SIDEBAR_WIDTH;
                self.settings.form_width = config::FORM_WIDTH;
                self.save_settings("Column widths reset");
            }
            Message::ToggleCompact => {
                self.settings.compact = !self.settings.compact;
                let what = if self.settings.compact {
                    "Compact rows"
                } else {
                    "Full rows"
                };
                self.save_settings(what);
            }
            Message::Tick => {
                let changed = self
                    .path
                    .as_ref()
                    .is_some_and(|p| storage::modified(p) != self.vault_mtime);
                if changed {
                    let task = self.update(Message::Reload);
                    if self.status.starts_with("Reloaded") {
                        self.status = self.status.replace('.', ", the file changed on disk.");
                    }
                    return task;
                }
            }
            Message::ToggleSidebar => {
                self.settings.sidebar = !self.settings.sidebar;
                let what = if self.settings.sidebar {
                    "Sidebar shown"
                } else {
                    "Sidebar hidden"
                };
                self.save_settings(what);
            }
            Message::Zoom(zoom) => {
                let zoom = zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
                self.settings.zoom = (zoom * 10.0).round() / 10.0;
                let percent = (self.settings.zoom * 100.0).round();
                self.save_settings(&format!("Zoom {percent}%"));
                let min_size = MIN_SIZE * self.settings.zoom;
                return window::latest()
                    .and_then(move |id| window::set_min_size(id, Some(min_size)));
            }
            Message::CloseRequested(id) => {
                if let Some(p) = &self.config_path
                    && let Err(e) = storage::save(p, &self.settings)
                {
                    log::warn!("could not save settings on close: {e}");
                }
                return window::close(id);
            }
            Message::Export => {
                let rows = self.visible();
                let count = rows.len();
                let md = export::markdown(&self.vault, &rows);
                let sh = export::shell(&self.vault, &rows);
                self.status = match self.path.as_ref().map(|p| p.with_file_name("export.md")) {
                    Some(p) => {
                        let written = storage::write_private(&p, &md)
                            .and_then(|()| storage::write_private(&p.with_extension("sh"), &sh));
                        match written {
                            Ok(()) => format!(
                                "Exported {count} commands to {} and .sh and copied.",
                                p.display()
                            ),
                            Err(e) => format!("Copied {count} commands, file write failed: {e}"),
                        }
                    }
                    None => {
                        format!("Copied Markdown for {count} commands, no directory to write to.")
                    }
                };
                return iced::clipboard::write(md.clone()).chain(self.clear_later(md));
            }
            Message::FocusSearch => return iced::widget::operation::focus(SEARCH_ID),
            Message::FocusNext => return iced::widget::operation::focus_next(),
            Message::FocusPrevious => return iced::widget::operation::focus_previous(),
            Message::Reset => {
                // The key panel closes first, then an open row, and the search survives both.
                if std::mem::take(&mut self.help) {
                    return Task::none();
                }
                if self.fill.take().is_some() || self.tag_rename.take().is_some() {
                    return iced::widget::operation::focus(SEARCH_ID);
                }
                self.search.clear();
                self.selected = None;
                self.reset_form();
                self.reset_category_form();
                return iced::widget::operation::focus(SEARCH_ID);
            }
            Message::SelectCategory(id) => {
                self.selected_category = id;
                // A new command or subcategory lands in the category being looked at.
                if self.form.editing.is_none() {
                    self.form.category_id = id;
                }
                if self.editing_category.is_none() {
                    self.new_category_parent = id;
                }
                // Remembered for the next start. Not worth a status line, so no save_settings.
                self.settings.category = id;
                if let Some(p) = &self.config_path
                    && let Err(e) = storage::save(p, &self.settings)
                {
                    log::warn!("could not save settings: {e}");
                }
            }
            Message::NewCategoryChanged(s) => self.new_category = s,
            Message::NewCategoryParent(c) => self.new_category_parent = c.id,
            Message::SubmitCategory => {
                let name = self.new_category.trim().to_owned();
                if name.is_empty() {
                    self.status = "Category name is required.".into();
                    return Task::none();
                }
                let done = match self.editing_category {
                    Some(_) => format!("Saved category \"{name}\"."),
                    None => format!("Added category \"{name}\"."),
                };
                match self.editing_category {
                    Some(id) => {
                        let before = self
                            .vault
                            .categories
                            .iter()
                            .find(|c| c.id == id)
                            .map(|c| (c.name.clone(), c.parent_id));
                        if !self
                            .vault
                            .update_category(id, name, self.new_category_parent)
                        {
                            self.status = "A category cannot be moved under itself.".into();
                            return Task::none();
                        }
                        if let Some((name, parent_id)) = before {
                            self.remember(Undo::CategoryChanged {
                                id,
                                name,
                                parent_id,
                            });
                        }
                    }
                    None => {
                        self.vault.add_category(name, self.new_category_parent);
                    }
                }
                self.reset_category_form();
                if self.persist() {
                    self.status = done;
                }
            }
            Message::EditCategory => {
                let selected = self
                    .selected_category
                    .and_then(|id| self.vault.categories.iter().find(|c| c.id == id));
                if let Some(c) = selected {
                    self.editing_category = Some(c.id);
                    self.new_category = c.name.clone();
                    self.new_category_parent = c.parent_id;
                    return iced::widget::operation::focus(CATEGORY_ID);
                }
            }
            Message::CancelCategory => self.reset_category_form(),
            Message::DeleteCategory => {
                let removed = self
                    .selected_category
                    .and_then(|id| self.vault.categories.iter().find(|c| c.id == id))
                    .cloned();
                if let Some(category) = removed {
                    let id = category.id;
                    let parent = category.parent_id;
                    let name = self.vault.path_of(Some(id));
                    let moved = self.vault.count_within(Some(id));
                    let children = self
                        .vault
                        .categories
                        .iter()
                        .filter(|c| c.parent_id == Some(id))
                        .map(|c| c.id)
                        .collect();
                    let commands = self
                        .vault
                        .commands
                        .iter()
                        .filter(|c| c.category_id == Some(id))
                        .map(|c| c.id)
                        .collect();
                    self.vault.remove_category(id);
                    self.remember(Undo::CategoryRemoved {
                        category,
                        children,
                        commands,
                    });
                    self.selected_category = parent;
                    if self.form.category_id == Some(id) {
                        self.form.category_id = parent;
                    }
                    if self.new_category_parent == Some(id) {
                        self.new_category_parent = parent;
                    }
                    if self.editing_category == Some(id) {
                        self.reset_category_form();
                    }
                    if self.persist() {
                        let commands = match moved {
                            1 => "1 command".to_owned(),
                            n => format!("{n} commands"),
                        };
                        self.status = format!(
                            "Removed \"{name}\". {commands} and any subcategories moved up, Undo puts them back."
                        );
                    }
                }
            }
        }
        Task::none()
    }

    /// Adds the frequent commands from `text` that are not in the vault yet, as one
    /// undo step. `source` names the file in the status line.
    fn import_history(&mut self, text: &str, source: &str) {
        let ranked = history::frequent(text, 2, 50);
        let added = history::import(&mut self.vault, &ranked, self.selected_category);
        let count = added.len();
        self.vault.ensure_tags();
        if count > 0 {
            self.remember(Undo::Imported(added));
        }
        if self.persist() {
            self.status = format!(
                "Imported {count} commands used twice or more from {source}. Undo removes them."
            );
        }
    }

    /// Counts the copy, highlights the row, saves, and hands `text` to the clipboard.
    fn copy_now(&mut self, id: Uuid, text: String) -> Task<Message> {
        if let Some(c) = self.vault.commands.iter_mut().find(|c| c.id == id) {
            c.copies += 1;
            c.last_copied = Some(chrono::Utc::now());
            let title = c.title.clone();
            self.selected = Some(id);
            if self.persist() {
                self.status = match self.settings.clear_after {
                    0 => format!("Copied \"{title}\"."),
                    secs => format!("Copied \"{title}\", clipboard clears in {secs} s."),
                };
            }
        }
        iced::clipboard::write(text.clone()).chain(self.clear_later(text))
    }

    /// Counts the copy and reads the clipboard back after `Settings.clear_after`
    /// seconds so the handler can wipe it while it still holds `copied`. A later
    /// copy moves the generation on, so an older timer wipes nothing, even a repeat
    /// of the same text keeps its full delay.
    fn clear_later(&mut self, copied: String) -> Task<Message> {
        self.copy_generation += 1;
        let generation = self.copy_generation;
        match self.settings.clear_after {
            0 => Task::none(),
            secs => after(Duration::from_secs(secs))
                .then(|()| iced::clipboard::read())
                .map(move |now| Message::ClipboardRead {
                    copied: copied.clone(),
                    now,
                    generation,
                }),
        }
    }

    /// Pushes an undo step and drops the oldest past `UNDO_DEPTH`.
    fn remember(&mut self, step: Undo) {
        self.undo.push(step);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
    }

    /// Title of another command with the same text, for a heads-up after saving.
    fn duplicate_of(&self, draft: &Draft) -> Option<String> {
        self.vault
            .commands
            .iter()
            .filter(|c| Some(c.id) != self.form.editing)
            .find(|c| c.command_text == draft.command_text)
            .map(|c| c.title.clone())
    }

    /// The highlighted row, only while the filter still shows it. A row hidden by a
    /// later search must not be edited or deleted from the keyboard.
    fn selected_visible(&self) -> Option<Uuid> {
        self.selected
            .filter(|s| self.visible().iter().any(|c| c.id == *s))
    }

    /// Clears the form but keeps the sidebar category as the default for the next command.
    fn reset_form(&mut self) {
        self.form = Form {
            category_id: self.selected_category,
            ..Form::default()
        };
    }

    fn reset_category_form(&mut self) {
        self.editing_category = None;
        self.new_category.clear();
    }

    /// Commands that pass the search and category filter, in the configured order.
    pub(crate) fn visible(&self) -> Vec<&Command> {
        let mut rows: Vec<&Command> = self
            .vault
            .commands
            .iter()
            .filter(|c| {
                c.matches(&self.search, &self.vault.path_of(c.category_id))
                    && self
                        .selected_category
                        .is_none_or(|a| self.vault.is_within(c.category_id, a))
            })
            .collect();
        match self.settings.sort {
            Sort::Title => rows.sort_by_cached_key(|c| c.title.to_lowercase()),
            Sort::Recent => rows.sort_by_key(|c| std::cmp::Reverse(c.updated_at)),
            Sort::Popular => {
                rows.sort_by_cached_key(|c| (std::cmp::Reverse(c.copies), c.title.to_lowercase()));
            }
            Sort::Copied => rows
                .sort_by_cached_key(|c| (std::cmp::Reverse(c.last_copied), c.title.to_lowercase())),
        }
        // Stable, so pinned rows keep the chosen order among themselves.
        rows.sort_by_key(|c| !c.pinned);
        rows
    }

    /// Writes `config.json` and puts `what` plus the outcome in the status line.
    fn save_settings(&mut self, what: &str) {
        self.status = match &self.config_path {
            Some(p) => match storage::save(p, &self.settings) {
                Ok(()) => format!("{what}."),
                Err(e) => format!("{what}, but settings could not be saved: {e}"),
            },
            None => format!("{what} for this session only."),
        };
    }

    /// The window's width in layout pixels, the minimum until the first resize arrives.
    fn window_width(&self) -> f32 {
        let s = &self.settings;
        s.window.map_or(MIN_SIZE.width, |w| w.width / s.zoom)
    }

    /// What the sidebar and the form may share once the list has `LIST_MIN`.
    fn room(&self) -> f32 {
        let gaps = if self.settings.sidebar { 2.0 } else { 1.0 };
        self.window_width() - 2.0 * EDGE - gaps * GAP - LIST_MIN
    }

    /// Sidebar and form widths to draw, 0 for a hidden sidebar. A window too narrow for
    /// the saved widths shrinks the form first, then the sidebar, down to their minimums.
    /// The saved widths stay, so a wider window brings them back.
    pub(crate) fn widths(&self) -> (f32, f32) {
        let (s, room) = (&self.settings, self.room());
        let sidebar = if s.sidebar {
            s.sidebar_width.min(room - FORM_MIN).max(SIDEBAR_MIN)
        } else {
            0.0
        };
        (sidebar, s.form_width.min(room - sidebar).max(FORM_MIN))
    }

    fn width(&self, divider: Divider) -> f32 {
        let (sidebar, form) = self.widths();
        match divider {
            Divider::Sidebar => sidebar,
            Divider::Form => form,
        }
    }

    /// `None` follows the system, so a switch on the desktop reaches a running app.
    pub(crate) fn theme(&self) -> Option<iced::Theme> {
        self.settings.scheme.theme()
    }

    /// Multiplied with the desktop's own scale factor by iced.
    pub(crate) fn scale(&self) -> f32 {
        self.settings.zoom
    }

    /// Keys through `on_event`, the file poll, plus window resizes and the close button.
    /// Pointer moves only while a gap is held, so plain mouse motion costs nothing.
    pub(crate) fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(
            [
                event::listen_with(on_event),
                Subscription::run(ticks),
                window::resize_events().map(|(_, size)| Message::Resized(size)),
                window::close_requests().map(Message::CloseRequested),
            ]
            .into_iter()
            .chain(self.dragging.map(|_| event::listen_with(on_drag))),
        )
    }

    /// Pick list entries: the `None` entry first, then the tree indented by depth.
    pub(crate) fn choices(&self, none_label: &str) -> Vec<Choice> {
        std::iter::once(Choice {
            id: None,
            label: none_label.into(),
        })
        .chain(self.vault.tree().into_iter().map(|(depth, c)| Choice {
            id: Some(c.id),
            label: format!("{}{}", "  ".repeat(depth), c.name),
        }))
        .collect()
    }

    /// Writes the whole vault after every change. Cheap at this size. Sets the status
    /// line and returns whether the save succeeded, so callers can put a more specific
    /// message over a plain "Saved". A file another program changed since the last
    /// load or save is kept beside the vault before it is overwritten.
    fn persist(&mut self) -> bool {
        let Some(p) = self.path.clone() else {
            self.status = "Change kept in memory only, saving is disabled.".into();
            return false;
        };
        let mut note = String::new();
        let on_disk = storage::modified(&p);
        if on_disk.is_some() && self.vault_mtime.is_some() && on_disk != self.vault_mtime {
            note = match storage::keep_copy(&p) {
                Ok(kept) => format!(
                    " The file had changed on disk since it was read, that version is kept as {}.",
                    storage::tidy(&kept)
                ),
                Err(e) => format!(" The file had changed on disk and could not be kept: {e}."),
            };
            log::warn!("vault changed on disk since the last load or save{note}");
        }
        match storage::save(&p, &self.vault) {
            Ok(()) => {
                log::debug!(
                    "saved {} commands to {}",
                    self.vault.commands.len(),
                    p.display()
                );
                self.vault_mtime = storage::modified(&p);
                self.status = format!("Saved.{note}");
                true
            }
            Err(e) => {
                log::error!("save failed: {e}");
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }
}

/// One `Tick` every few seconds for the file poll. A std thread sleeps and sends,
/// iced's default executor has no timers, and it stops when the receiver is gone.
fn ticks() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    let (mut tx, rx) = iced::futures::channel::mpsc::channel(1);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(POLL);
            if tx.try_send(()).is_err() && tx.is_closed() {
                break;
            }
        }
    });
    rx.map(|()| Message::Tick)
}

/// The widget id of a row's container, so `reveal` can find it in the tree.
pub(crate) fn row_id(id: Uuid) -> Id {
    Id::from(id.to_string())
}

/// Scrolls the list the least distance that shows the row, after measuring it.
fn reveal(row: Uuid) -> Task<Message> {
    let measure = Measure {
        list: Id::new(LIST_ID),
        row: row_id(row),
        viewport: None,
        found: None,
    };
    iced::advanced::widget::operate(measure).then(|offset| match offset {
        Some(y) => iced::widget::operation::scroll_to(
            LIST_ID,
            AbsoluteOffset {
                x: None,
                y: Some(y),
            },
        ),
        None => Task::none(),
    })
}

/// Where the list must scroll to show a row, `None` when it already does. `offset`
/// is the current scroll, `view` the viewport height, `top` and `height` the row's
/// place in the content. A row taller than the view shows its top.
fn reveal_offset(offset: f32, view: f32, top: f32, height: f32) -> Option<f32> {
    let bottom = top + height;
    if top < offset {
        Some(top)
    } else if bottom > offset + view {
        Some((bottom - view).min(top))
    } else {
        None
    }
}

/// One walk of the widget tree for the list's viewport and scroll and the row's
/// bounds. The scrollable is visited before its rows and its state is only lent
/// during that visit, so the scroll itself is a second task, see `reveal`.
struct Measure {
    list: Id,
    row: Id,
    /// Content top, current scroll, viewport height.
    viewport: Option<(f32, f32, f32)>,
    /// Row top and height, in the same unscrolled coordinates as the content.
    found: Option<(f32, f32)>,
}

impl Operation<Option<f32>> for Measure {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<f32>>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.list) {
            self.viewport = Some((content.y, translation.y, bounds.height));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.row) {
            self.found = Some((bounds.y, bounds.height));
        }
    }

    fn finish(&self) -> Outcome<Option<f32>> {
        match (self.viewport, self.found) {
            (Some((content_top, offset, view)), Some((top, height))) => {
                Outcome::Some(reveal_offset(offset, view, top - content_top, height))
            }
            _ => Outcome::None,
        }
    }
}

/// Keys no widget consumed, the focused search box passes Up and Down through,
/// which is what `keyboard::listen` gives. F1 comes through captured or not, the
/// key panel sits over every widget, and a captured Escape only closes that panel.
/// A left-button release ends any drag. It is heard here rather than in `on_drag`,
/// which starts a frame late and could miss a quick click's release.
fn on_event(event: Event, status: event::Status, _window: window::Id) -> Option<Message> {
    let pressed = match event {
        Event::Keyboard(pressed) => pressed,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            return Some(Message::DragEnd);
        }
        _ => return None,
    };
    let named = match &pressed {
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named),
            ..
        } => Some(*named),
        _ => None,
    };
    match (named, status) {
        (Some(key::Named::F1), _) => Some(Message::ToggleHelp),
        (Some(key::Named::Escape), event::Status::Captured) => Some(Message::CloseHelp),
        (_, event::Status::Ignored) => Some(Message::Key(pressed)),
        _ => None,
    }
}

/// Pointer moves while a gap is held. The pointer leaves the gap as soon as it moves,
/// so the gap's own `mouse_area` cannot follow it. The window keeps receiving pointer
/// events until the release, even with the pointer outside it.
fn on_drag(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => Some(Message::DragTo(position.x)),
        _ => None,
    }
}

/// Completes after `wait` on a spare thread. iced's default executor is a thread
/// pool without timers, and a sleeping worker would block it, so the wait gets its
/// own thread and hands over through a oneshot.
fn after(wait: Duration) -> Task<()> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(wait);
        let _ = tx.send(());
    });
    Task::future(async move {
        let _ = rx.await;
    })
}

/// Next selected id after moving `delta` rows, clamped at both ends. Nothing selected,
/// or a selection no longer visible, counts as sitting before the first row, so Down
/// and Home land on the first row and End on the last.
fn step(ids: &[Uuid], current: Option<Uuid>, delta: isize) -> Option<Uuid> {
    let last = ids.len().checked_sub(1)?;
    let at = current
        .and_then(|s| ids.iter().position(|&i| i == s))
        .map_or(-1, |i| i as isize);
    let next = at.saturating_add(delta).clamp(0, last as isize);
    Some(ids[next as usize])
}

#[cfg(test)]
mod tests;
