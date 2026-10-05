use iced::border;
use iced::keyboard::{self, key};
use iced::mouse;
use iced::widget::{
    Column, Row, TextInput, button, center, column, container, mouse_area, opaque, pick_list, row,
    rule, scrollable, stack, text, text_editor, text_input, tooltip,
};
use iced::{Alignment, Color, Element, Font, Length, Padding, Theme};
use uuid::Uuid;

use crate::app::{
    App, CATEGORY_ID, Divider, EDGE, FILL_ID, GAP, LIST_ID, Message, SEARCH_ID, TAG_RENAME_ID,
    TITLE_ID, row_id,
};
use crate::config::{Scheme, Sort};
use crate::model;

impl App {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let (sidebar_width, form_width) = self.widths();
        let sidebar = self
            .settings
            .sidebar
            .then(|| [self.view_sidebar(sidebar_width), divider(Divider::Sidebar)]);
        let sidebar_toggle = if self.settings.sidebar {
            "Hide sidebar"
        } else {
            "Show sidebar"
        };

        let rows = self.visible();
        let results = rows.iter().copied().map(|c| {
            // The title fills and wraps, so the buttons stay in view at the minimum
            // window width. Path and copy count go on their own small line.
            // A title derived from the command would show the same text twice.
            let title_is_command = c.title == c.command_text;
            let title = if title_is_command {
                text(&c.title).size(18).font(Font::MONOSPACE)
            } else {
                text(&c.title).size(18)
            };
            let star = if c.pinned { "\u{2605}" } else { "\u{2606}" };
            let stamp = |t: &chrono::DateTime<chrono::Utc>| {
                t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
            };
            let now = chrono::Utc::now();
            let copied = match c.last_copied {
                Some(t) => format!("Copied {} times, last {}.", c.copies, model::ago(t, now)),
                None => "Never copied.".into(),
            };
            let facts = format!(
                "Updated {}, {}. Created {}. {copied}",
                model::ago(c.updated_at, now),
                stamp(&c.updated_at),
                stamp(&c.created_at),
            );
            let header = row![
                hint(title.width(Length::Fill), facts),
                button(text(star).size(20))
                    .style(button::text)
                    .on_press(Message::TogglePin(c.id)),
                button("Copy").on_press(Message::Copy(c.id)),
                button("Edit").on_press(Message::Edit(c.id)),
                button("Delete").on_press(Message::Delete(c.id)),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            let path = self.vault.path_of(c.category_id);
            let meta: Vec<String> = [Some(path).filter(|p| !p.is_empty()), copies_label(c.copies)]
                .into_iter()
                .flatten()
                .collect();
            // Compact rows keep the title and the command, the rest waits in the tooltip.
            let full = !self.settings.compact;
            let body = column![header]
                .extend(
                    (!title_is_command).then(|| text(&c.command_text).font(Font::MONOSPACE).into()),
                )
                .extend(
                    (full && !c.description.is_empty())
                        .then(|| text(&c.description).size(13).into()),
                )
                .extend(
                    (full && !meta.is_empty())
                        .then(|| text(meta.join("  \u{b7}  ")).size(12).into()),
                )
                .extend((full && !c.tags.is_empty()).then(|| {
                    Row::with_children(c.tags.iter().map(|t| chip(t, self.vault.tag_colour(t))))
                        .spacing(4)
                        .into()
                }))
                .spacing(4);
            let style: fn(&Theme) -> container::Style = if self.selected == Some(c.id) {
                container::rounded_box
            } else {
                container::transparent
            };
            mouse_area(container(body).id(row_id(c.id)).padding(6).style(style))
                .on_press(Message::Select(c.id))
                .on_double_click(Message::Copy(c.id))
                .into()
        });
        let empty = match (self.vault.commands.is_empty(), !rows.is_empty()) {
            (true, _) => Some(
                "No commands yet. Add one on the right, or press Import for the ones you use most.",
            ),
            (false, false) => Some("Nothing matches the search and category."),
            _ => None,
        };
        let list = scrollable(
            Column::with_children(results)
                .extend(empty.map(|t| text(t).size(14).into()))
                .spacing(16)
                .width(Length::Fill),
        )
        .id(LIST_ID);

        let (heading, submit) = match self.form.editing {
            Some(_) => ("Edit command", "Save"),
            None => ("Add command", "Add"),
        };
        let form_choices = self.choices("No category");
        let form_selected = form_choices
            .iter()
            .find(|c| c.id == self.form.category_id)
            .cloned();
        let form = column![
            text(heading).size(18),
            field("Title", &self.form.title, Message::TitleChanged).id(TITLE_ID),
            text_editor(&self.form.command)
                .placeholder("Command, Shift+Enter newline")
                .on_action(Message::CommandAction)
                .font(Font::MONOSPACE)
                .key_binding(submit_on_enter),
            text_editor(&self.form.description)
                .placeholder("Description, Shift+Enter newline")
                .on_action(Message::DescriptionAction)
                .key_binding(submit_on_enter),
            field(
                "Tags, comma separated",
                &self.form.tags,
                Message::TagsChanged
            ),
            pick_list(form_choices, form_selected, Message::FormCategory).width(Length::Fill),
            row![button(submit).on_press(Message::Submit)]
                .extend(
                    self.form
                        .editing
                        .map(|_| button("Cancel").on_press(Message::Cancel).into())
                )
                .spacing(8),
        ]
        .spacing(8)
        .width(form_width);

        let status_row: Element<'_, Message> = if let Some(r) = &self.tag_rename {
            row![
                text(format!("Rename tag \"{}\" to", r.old)).size(12),
                text_input("New name", &r.text)
                    .id(TAG_RENAME_ID)
                    .on_input(Message::TagRenameChanged)
                    .on_submit(Message::TagRenameSubmit)
                    .width(200),
                button("Save").on_press(Message::TagRenameSubmit),
                button("Cancel").on_press(Message::TagRenameCancel),
                button("Remove")
                    .style(button::danger)
                    .on_press(Message::TagRemove),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        } else if let Some(f) = &self.fill {
            let inputs = f
                .names
                .iter()
                .zip(&f.values)
                .enumerate()
                .map(|(i, (name, value))| {
                    let input = text_input(name, value)
                        .on_input(move |v| Message::FillChanged(i, v))
                        .on_submit(Message::FillSubmit)
                        .width(160);
                    if i == 0 { input.id(FILL_ID) } else { input }.into()
                });
            let inputs = row![text("Fill in").size(12)]
                .extend(inputs)
                .push(button("Copy").on_press(Message::FillSubmit))
                .push(hint(
                    button("As is").on_press(Message::FillRaw),
                    "Copy the command with its markers as written",
                ))
                .push(button("Cancel").on_press(Message::FillCancel))
                .spacing(8)
                .align_y(Alignment::Center);
            // Many placeholders overrun the minimum window width, so this row scrolls.
            scrollable(inputs)
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new(),
                ))
                .into()
        } else {
            row![text(&self.status).size(12).width(Length::Fill)]
                .extend(
                    self.undo
                        .last()
                        .map(|_| button("Undo").on_press(Message::Undo).into()),
                )
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
        };

        let page: Element<'_, Message> =
            column![
            row![
                text_input("Search. Up and Down select, Enter copies", &self.search)
                    .id(SEARCH_ID)
                    .on_input(Message::SearchChanged)
                    .on_submit(Message::CopySelected),
                pick_list(Sort::ALL, Some(self.settings.sort), Message::SortChanged),
                // Fixed, or the longest theme name sets the width and squeezes the search.
                pick_list(
                    Scheme::all(),
                    Some(self.settings.scheme.clone()),
                    Message::SchemeChanged
                )
                .width(190),
                hint(
                    button("Export").on_press(Message::Export),
                    "The visible commands as Markdown to export.md and the clipboard, and as a script to export.sh",
                ),
                hint(
                    button("Import").on_press(Message::ImportHistory),
                    "The 50 most used commands in your shell history, into the selected category",
                ),
                hint(
                    button(sidebar_toggle).on_press(Message::ToggleSidebar),
                    "Categories and tags, Ctrl+B",
                ),
                hint(button("Keys").on_press(Message::ToggleHelp), "Every key, F1"),
            ]
            .spacing(12),
            Row::with_children(sidebar.into_iter().flatten())
                .push(list)
                .push(divider(Divider::Form))
                .push(form)
                .height(Length::Fill),
            status_row,
        ]
            .spacing(12)
            .padding(EDGE)
            .into();
        if !self.help {
            return page;
        }
        let keys = KEYS.iter().map(|(keys, what)| {
            row![
                text(*keys).size(13).width(170),
                text(*what).size(13).width(Length::Fill)
            ]
            .spacing(12)
            .into()
        });
        let panel = container(
            column![
                text("Keys").size(18),
                // Right padding keeps the last words out from under the scrollbar.
                scrollable(Column::with_children(keys).spacing(4).padding(Padding {
                    top: 0.0,
                    right: 16.0,
                    bottom: 0.0,
                    left: 0.0,
                }))
                .height(Length::Fill),
            ]
            .spacing(12),
        )
        .padding(16)
        .width(520)
        .height(Length::Fill)
        .style(container::rounded_box);
        let shade = |_: &Theme| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.5).into()),
            ..container::Style::default()
        };
        stack([
            page,
            opaque(
                mouse_area(center(opaque(panel)).padding(40).style(shade))
                    .on_press(Message::ToggleHelp),
            ),
        ])
        .into()
    }

    fn view_sidebar(&self, width: f32) -> Element<'_, Message> {
        let look = |id: Option<Uuid>| {
            if id == self.selected_category {
                Look::Selected
            } else if self.hovered == Some(id) {
                Look::Hovered
            } else {
                Look::Plain
            }
        };
        let mut tree = column![category_entry(
            "All",
            None,
            0,
            look(None),
            self.vault.count_within(None),
        )]
        .spacing(2)
        .padding(BAR_ROOM);
        for (depth, c) in self.vault.tree() {
            tree = tree.push(category_entry(
                &c.name,
                Some(c.id),
                depth,
                look(Some(c.id)),
                self.vault.count_within(Some(c.id)),
            ));
        }

        let mut parent_choices = self.choices("Top level");
        if let Some(editing) = self.editing_category {
            // A category cannot become its own descendant.
            parent_choices.retain(|c| {
                !c.id
                    .is_some_and(|id| self.vault.is_within(Some(id), editing))
            });
        }
        let parent_selected = parent_choices
            .iter()
            .find(|c| c.id == self.new_category_parent)
            .cloned();
        let (placeholder, actions) = if self.editing_category.is_some() {
            (
                "Category name",
                row![
                    button("Save").on_press(Message::SubmitCategory),
                    button("Cancel").on_press(Message::CancelCategory),
                ],
            )
        } else {
            let mut actions = row![button("Add").on_press(Message::SubmitCategory)];
            if self.selected_category.is_some() {
                actions = actions
                    .push(button("Edit").on_press(Message::EditCategory))
                    .push(
                        button("Delete")
                            .style(button::danger)
                            .on_press(Message::DeleteCategory),
                    );
            }
            ("New category", actions)
        };
        let form = column![
            text_input(placeholder, &self.new_category)
                .id(CATEGORY_ID)
                .on_input(Message::NewCategoryChanged)
                .on_submit(Message::SubmitCategory),
            pick_list(parent_choices, parent_selected, Message::NewCategoryParent)
                .width(Length::Fill),
            actions.spacing(8),
        ]
        .spacing(8);

        let tags = self.vault.tag_usage();
        let tag_list = Column::with_children(tags.iter().map(|(t, n)| {
            row![chip(&t.name, Some(&t.colour)), text(n.to_string()).size(11)]
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
        }))
        .spacing(4)
        .width(Length::Fill)
        .padding(BAR_ROOM);
        // The tags get their own scrollable with a ceiling, so a long tree cannot push
        // them out of sight and a long tag list cannot squeeze the tree.
        column![
            text("Categories").size(18),
            scrollable(tree).height(Length::Fill),
        ]
        .extend((!tags.is_empty()).then(|| text("Tags").size(18).into()))
        .extend((!tags.is_empty()).then(|| container(scrollable(tag_list)).max_height(160).into()))
        .push(form)
        .spacing(8)
        .width(width)
        .into()
    }
}

/// The key panel's rows. The first column must match a Key cell in the README
/// table, a test checks that.
const KEYS: &[(&str, &str)] = &[
    ("Typing", "Filter the list"),
    ("Up, Down", "Move the highlight"),
    ("PageUp, PageDown", "Move it ten rows"),
    ("Home, End", "First or last row"),
    ("Alt+Up, Alt+Down", "Previous or next category"),
    ("Enter", "Copy the highlight, or submit a form"),
    ("Tab, Shift+Tab", "Next or previous text field"),
    ("Escape", "Close this or an open row, then clear the search"),
    ("Ctrl+Z", "Undo, up to twenty steps"),
    ("Ctrl+Up, Ctrl+Down", "Zoom"),
    ("Ctrl+B", "Hide or show the sidebar"),
    ("Ctrl+Shift+B", "Compact or full rows"),
    ("Ctrl+F", "Jump to the search box"),
    ("Ctrl+R", "Reload the vault from disk"),
    ("Ctrl+N", "New command"),
    ("Ctrl+E", "Edit the highlight"),
    ("Ctrl+Shift+E", "Duplicate the highlight into the form"),
    ("Ctrl+D", "Delete the highlight"),
    ("Ctrl+P", "Pin or unpin the highlight"),
    ("F2", "Rename or move the selected category"),
    ("F1", "Show or hide these keys"),
    ("Click a row", "Highlight it, double-click copies"),
    ("Click a category", "Filter by it, double-click edits"),
    ("Click a tag chip", "Search for the tag"),
    ("Right-click a tag chip", "Next palette colour"),
    ("Double-click a tag chip", "Rename, merge or remove the tag"),
];

/// The search that finds exactly this tag, quoted when the name has spaces.
fn tag_query(name: &str) -> String {
    if name.contains(char::is_whitespace) {
        format!("\"tag:{name}\"")
    } else {
        format!("tag:{name}")
    }
}

/// Right padding inside the sidebar scrollables, so the bar does not cover the counts.
const BAR_ROOM: Padding = Padding {
    top: 0.0,
    right: 12.0,
    bottom: 0.0,
    left: 0.0,
};

/// How a sidebar entry is drawn: the selected filter, under the pointer, or neither.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Selected,
    Hovered,
    Plain,
}

/// A sidebar entry with its command count, indented by depth. A click selects it, a
/// double-click loads it into the form, and the pointer over it reports for the tint.
fn category_entry<'a>(
    label: &'a str,
    id: Option<Uuid>,
    depth: usize,
    look: Look,
    count: usize,
) -> Element<'a, Message> {
    // A button would capture the press before the mouse area saw it, hence a container.
    let style: fn(&Theme) -> container::Style = match look {
        Look::Selected => selected_entry,
        Look::Hovered => hovered_entry,
        Look::Plain => container::transparent,
    };
    let entry = container(
        row![
            text(label).width(Length::Fill),
            text(count.to_string()).size(12)
        ]
        .align_y(Alignment::Center),
    )
    .style(style)
    .width(Length::Fill)
    .padding(Padding {
        top: 5.0,
        right: 10.0,
        bottom: 5.0,
        left: 10.0 + 12.0 * depth as f32,
    });
    mouse_area(entry)
        .on_press(Message::SelectCategory(id))
        .on_double_click(Message::EditCategory)
        .on_enter(Message::HoverCategory(Some(id)))
        .on_exit(Message::HoverCategory(None))
        .into()
}

/// The faint tint `button::text` gave a hovered entry.
fn hovered_entry(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.extended_palette().background.weak.color.into()),
        border: border::rounded(2),
        ..container::Style::default()
    }
}

/// The primary colours, what `button::primary` gave the selected entry.
fn selected_entry(theme: &Theme) -> container::Style {
    let primary = theme.extended_palette().primary.base;
    container::Style {
        background: Some(primary.color.into()),
        text_color: Some(primary.text),
        border: border::rounded(2),
        ..container::Style::default()
    }
}

/// A tooltip under a widget, for buttons whose one word does not say enough.
fn hint<'a>(
    content: impl Into<Element<'a, Message>>,
    words: impl text::IntoFragment<'a>,
) -> Element<'a, Message> {
    tooltip(content, text(words).size(12), tooltip::Position::Bottom)
        .style(container::rounded_box)
        .padding(6)
        .into()
}

/// The gap beside the list, with a line down its middle. Drag it to resize the column
/// on the far side, double-click it for the default widths.
fn divider<'a>(which: Divider) -> Element<'a, Message> {
    mouse_area(
        container(rule::vertical(1))
            .center_x(GAP)
            .height(Length::Fill),
    )
    .interaction(mouse::Interaction::ResizingHorizontally)
    .on_press(Message::DragStart(which))
    .on_double_click(Message::ResetWidths)
    .into()
}

/// "1 copy" or "n copies", nothing while a command has never been copied.
fn copies_label(n: u32) -> Option<String> {
    match n {
        0 => None,
        1 => Some("1 copy".into()),
        n => Some(format!("{n} copies")),
    }
}

/// Enter submits the form like the other fields, Shift+Enter starts a new line, so a
/// script fits in the command editor without losing the one-key submit. Ctrl+Up and
/// Ctrl+Down zoom here as everywhere else, the editor would otherwise move the cursor.
fn submit_on_enter(press: text_editor::KeyPress) -> Option<text_editor::Binding<Message>> {
    use text_editor::Binding;
    // iced calls this for every key press in the window. Only a focused editor may act.
    if !matches!(press.status, text_editor::Status::Focused { .. }) {
        return None;
    }
    let (ctrl, shift) = (press.modifiers.control(), press.modifiers.shift());
    match press.key {
        keyboard::Key::Named(key::Named::Enter) if !shift => Some(Binding::Custom(Message::Submit)),
        keyboard::Key::Named(key::Named::ArrowUp) if ctrl => {
            Some(Binding::Custom(Message::ZoomStep(1)))
        }
        keyboard::Key::Named(key::Named::ArrowDown) if ctrl => {
            Some(Binding::Custom(Message::ZoomStep(-1)))
        }
        _ => Binding::from_key_press(press),
    }
}

/// A form field. Enter submits the form from any of them.
fn field<'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: fn(String) -> Message,
) -> TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .on_submit(Message::Submit)
}

/// A tag chip in its registry colour. Clicking it searches for the tag.
fn chip<'a>(name: &'a str, colour: Option<&str>) -> Element<'a, Message> {
    let background = colour
        .and_then(parse_hex)
        .unwrap_or(Color::from_rgb(0.4, 0.4, 0.4));
    let label = container(text(name).size(11).color(Color::WHITE))
        .padding([2, 6])
        .style(move |_theme: &Theme| container::Style {
            background: Some(background.into()),
            border: border::rounded(8),
            ..container::Style::default()
        });
    let chip = mouse_area(label)
        .on_press(Message::SearchChanged(tag_query(name)))
        .on_right_press(Message::CycleTagColour(name.to_owned()))
        .on_double_click(Message::TagRenameStart(name.to_owned()));
    hint(
        chip,
        "Click to search, right-click for the next colour, double-click to rename or remove",
    )
}

/// `#rrggbb` to a colour. Anything else is `None` and the chip falls back to grey.
fn parse_hex(hex: &str) -> Option<Color> {
    let digits = hex.strip_prefix('#').filter(|d| d.len() == 6)?;
    let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    let (r, g, b) = (channel(0)?, channel(2)?, channel(4)?);
    Some(Color::from_rgb8(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_in_the_panel_has_a_row_in_the_readme() {
        let readme = include_str!("../README.md");
        for (keys, _) in KEYS {
            assert!(
                readme.contains(&format!("| {keys} |")),
                "{keys} is not in the README table"
            );
        }
    }

    #[test]
    fn hex_parses_only_six_digit_colours() {
        assert_eq!(parse_hex("#ff0080"), Some(Color::from_rgb8(255, 0, 128)));
        assert_eq!(parse_hex("ff0080"), None);
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("#gg0000"), None);
    }

    #[test]
    fn copies_label_handles_none_one_and_many() {
        assert_eq!(copies_label(0), None);
        assert_eq!(copies_label(1).as_deref(), Some("1 copy"));
        assert_eq!(copies_label(3).as_deref(), Some("3 copies"));
    }
}
