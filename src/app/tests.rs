use super::*;

#[test]
fn step_clamps_and_starts_at_first() {
    let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
    assert_eq!(step(&[], None, 1), None);
    assert_eq!(step(&ids, None, 1), Some(ids[0]));
    assert_eq!(step(&ids, None, -1), Some(ids[0]));
    assert_eq!(step(&ids, Some(ids[0]), -1), Some(ids[0]));
    assert_eq!(step(&ids, Some(ids[0]), 1), Some(ids[1]));
    assert_eq!(step(&ids, Some(ids[2]), 1), Some(ids[2]));
    assert_eq!(step(&ids, Some(Uuid::new_v4()), 1), Some(ids[0]));
    assert_eq!(
        step(&ids, None, isize::MAX),
        Some(ids[2]),
        "End from nothing"
    );
    assert_eq!(step(&ids, Some(ids[2]), isize::MIN), Some(ids[0]), "Home");
    assert_eq!(
        step(&ids, Some(ids[0]), PAGE),
        Some(ids[2]),
        "a page past the end"
    );
}

#[test]
fn reveal_offset_moves_the_least_that_shows_the_row() {
    assert_eq!(
        reveal_offset(0.0, 100.0, 50.0, 20.0),
        None,
        "already in view"
    );
    assert_eq!(
        reveal_offset(0.0, 100.0, 80.0, 20.0),
        None,
        "ends at the edge"
    );
    assert_eq!(
        reveal_offset(0.0, 100.0, 150.0, 20.0),
        Some(70.0),
        "below: its bottom meets the view's bottom"
    );
    assert_eq!(
        reveal_offset(200.0, 100.0, 150.0, 20.0),
        Some(150.0),
        "above: its top meets the view's top"
    );
    assert_eq!(
        reveal_offset(0.0, 100.0, 150.0, 300.0),
        Some(150.0),
        "taller than the view: show its top"
    );
}

fn app() -> App {
    App {
        path: None,
        vault_mtime: None,
        vault: Vault::default(),
        config_path: None,
        settings: Settings::default(),
        search: String::new(),
        selected: None,
        selected_category: None,
        new_category: String::new(),
        new_category_parent: None,
        editing_category: None,
        form: Form::default(),
        status: String::new(),
        undo: Vec::new(),
        tag_rename: None,
        fill: None,
        fill_memory: HashMap::new(),
        help: false,
        copy_generation: 0,
        hovered: None,
    }
}

fn send(app: &mut App, messages: impl IntoIterator<Item = Message>) {
    for m in messages {
        let _ = app.update(m);
    }
}

/// Pastes `text` into the command editor, the test's way of typing a command.
fn command(text: &str) -> Message {
    Message::CommandAction(text_editor::Action::Edit(text_editor::Edit::Paste(
        std::sync::Arc::new(text.into()),
    )))
}

fn key(key: keyboard::Key, ctrl: bool) -> Message {
    Message::Key(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: keyboard::Location::Standard,
        modifiers: if ctrl {
            keyboard::Modifiers::CTRL
        } else {
            keyboard::Modifiers::empty()
        },
        text: None,
        repeat: false,
    })
}

#[test]
fn ctrl_shift_e_duplicates_the_highlight_into_the_form() {
    let mut app = app();
    send(
        &mut app,
        [
            Message::TitleChanged("List".into()),
            command("ls -la"),
            Message::TagsChanged("fs".into()),
            Message::Submit,
        ],
    );
    let id = app.vault.commands[0].id;
    let mut shifted = key(keyboard::Key::Character("E".into()), true);
    if let Message::Key(keyboard::Event::KeyPressed { modifiers, .. }) = &mut shifted {
        *modifiers = keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT;
    }
    send(&mut app, [Message::Select(id), shifted]);
    assert_eq!(app.form.editing, None, "a new command, not an edit");
    assert_eq!(app.form.title, "List");
    assert_eq!(app.form.tags, "fs");
    send(
        &mut app,
        [Message::TitleChanged("List again".into()), Message::Submit],
    );
    assert_eq!(app.vault.commands.len(), 2);
    assert_eq!(
        app.vault.commands[0].title, "List",
        "the original is untouched"
    );
}

#[test]
fn keys_edit_delete_and_start_a_new_command_from_the_highlight() {
    let mut app = app();
    send(
        &mut app,
        [
            Message::TitleChanged("t".into()),
            command("c"),
            Message::Submit,
        ],
    );
    let id = app.vault.commands[0].id;
    send(&mut app, [key(keyboard::Key::Character("e".into()), true)]);
    assert_eq!(
        app.form.editing, None,
        "nothing highlighted, nothing to edit"
    );

    send(
        &mut app,
        [
            Message::Move(1),
            key(keyboard::Key::Character("e".into()), true),
        ],
    );
    assert_eq!(app.form.editing, Some(id));
    send(&mut app, [key(keyboard::Key::Character("n".into()), true)]);
    assert_eq!(app.form.editing, None, "Ctrl+N clears the form");

    send(
        &mut app,
        [
            Message::SearchChanged("zzz".into()),
            key(keyboard::Key::Named(key::Named::Delete), false),
        ],
    );
    assert_eq!(
        app.vault.commands.len(),
        1,
        "a hidden highlight is left alone"
    );
    send(
        &mut app,
        [
            Message::SearchChanged(String::new()),
            key(keyboard::Key::Character("d".into()), false),
        ],
    );
    assert_eq!(app.vault.commands.len(), 1, "plain d is typing, not delete");
    send(&mut app, [key(keyboard::Key::Character("d".into()), true)]);
    assert!(app.vault.commands.is_empty());
    assert!(!app.undo.is_empty(), "undo still covers a keyboard delete");
}

#[test]
fn add_edit_delete_through_messages() {
    let mut app = app();
    send(&mut app, [Message::Submit]);
    assert!(app.vault.commands.is_empty(), "empty form must not add");

    send(
        &mut app,
        [
            Message::TitleChanged("List".into()),
            command(" ls -la "),
            Message::TagsChanged("fs, , basics".into()),
            Message::Submit,
        ],
    );
    assert_eq!(app.vault.commands.len(), 1);
    let c = &app.vault.commands[0];
    assert_eq!(c.command_text, "ls -la");
    assert_eq!(c.tags, ["fs", "basics"]);
    assert!(
        app.vault.tag_colour("basics").is_some(),
        "tags get a colour on submit"
    );
    assert!(app.form.title.is_empty(), "form resets after submit");
    let id = c.id;

    send(&mut app, [Message::Edit(id)]);
    assert_eq!(app.form.editing, Some(id));
    send(
        &mut app,
        [Message::TitleChanged("Listing".into()), Message::Submit],
    );
    assert_eq!(app.vault.commands.len(), 1);
    assert_eq!(app.vault.commands[0].title, "Listing");

    send(&mut app, [Message::CopySelected]);
    assert_eq!(
        app.selected,
        Some(id),
        "Enter with nothing highlighted shows what it copied"
    );

    send(&mut app, [Message::Delete(id)]);
    assert!(app.vault.commands.is_empty());
    assert_eq!(app.selected, None);

    send(&mut app, [command("git status\ngit diff"), Message::Submit]);
    assert_eq!(
        app.vault.commands[0].title, "git status",
        "title from the command"
    );
}

#[test]
fn new_command_inherits_selected_category_and_filter_applies() {
    let mut app = app();
    let linux = app.vault.add_category("linux".into(), None);
    let windows = app.vault.add_category("windows".into(), None);
    send(
        &mut app,
        [
            Message::SelectCategory(Some(linux)),
            Message::TitleChanged("t".into()),
            command("c"),
            Message::Submit,
        ],
    );
    assert_eq!(app.vault.commands[0].category_id, Some(linux));
    assert_eq!(
        app.new_category_parent,
        Some(linux),
        "the sidebar form follows too"
    );
    assert_eq!(
        app.settings.category,
        Some(linux),
        "remembered for the next start"
    );
    assert_eq!(app.visible().len(), 1);
    send(&mut app, [Message::SelectCategory(Some(windows))]);
    assert_eq!(app.visible().len(), 0);
    send(&mut app, [Message::SelectCategory(None)]);
    assert_eq!(app.visible().len(), 1);
}

#[test]
fn reset_clears_search_and_highlight_but_keeps_category() {
    let mut app = app();
    let linux = app.vault.add_category("linux".into(), None);
    send(
        &mut app,
        [
            Message::TitleChanged("t".into()),
            command("c"),
            Message::Submit,
            Message::SelectCategory(Some(linux)),
            Message::SearchChanged("zzz".into()),
            Message::Move(1),
        ],
    );
    let id = app.vault.commands[0].id;
    send(&mut app, [Message::Edit(id), Message::Reset]);
    assert!(app.search.is_empty());
    assert_eq!(app.selected, None);
    assert_eq!(app.selected_category, Some(linux));
    assert_eq!(app.form.editing, None, "Escape cancels an edit");
}

#[test]
fn undo_restores_the_last_deleted_command() {
    let mut app = app();
    send(
        &mut app,
        [
            Message::TitleChanged("t".into()),
            command("c"),
            Message::TagsChanged("x".into()),
            Message::Submit,
        ],
    );
    let id = app.vault.commands[0].id;
    send(&mut app, [Message::CopySelected]);
    assert_eq!(
        app.selected,
        Some(id),
        "Enter with nothing highlighted shows what it copied"
    );

    send(&mut app, [Message::Delete(id)]);
    assert!(app.vault.commands.is_empty());
    assert_eq!(app.selected, None);
    assert!(!app.undo.is_empty());

    send(&mut app, [Message::Undo]);
    assert_eq!(app.vault.commands.len(), 1);
    assert_eq!(app.vault.commands[0].id, id);
    assert!(app.undo.is_empty());
    send(&mut app, [Message::Undo]);
    assert_eq!(app.vault.commands.len(), 1, "second undo is a no-op");
}

#[test]
fn copy_asks_for_placeholder_values_first() {
    let mut app = app();
    send(
        &mut app,
        [command("ssh {{user}}@{{host=box}}"), Message::Submit],
    );
    let id = app.vault.commands[0].id;
    send(&mut app, [Message::Copy(id)]);
    let f = app.fill.as_ref().expect("a fill row opens");
    assert_eq!(f.names, ["user", "host"]);
    assert_eq!(f.values, ["", "box"], "a default is prefilled");
    assert_eq!(app.vault.commands[0].copies, 0, "nothing copied yet");
    assert_eq!(app.selected, Some(id));
    send(
        &mut app,
        [
            Message::FillChanged(0, "me".into()),
            Message::FillChanged(1, "box".into()),
            Message::FillChanged(9, "ignored".into()),
            Message::FillSubmit,
        ],
    );
    assert!(app.fill.is_none());
    assert_eq!(app.vault.commands[0].copies, 1);
    assert_eq!(
        app.vault.commands[0].command_text, "ssh {{user}}@{{host=box}}",
        "the stored command keeps its markers"
    );

    send(
        &mut app,
        [
            Message::SearchChanged("ssh".into()),
            Message::Copy(id),
            Message::Reset,
        ],
    );
    assert!(app.fill.is_none(), "Escape closes the row");
    assert_eq!(app.search, "ssh", "and keeps the search");
    send(&mut app, [Message::Reset]);
    assert!(app.search.is_empty(), "the next Escape clears it");
    send(&mut app, [Message::Copy(id)]);
    let f = app.fill.as_ref().expect("a second fill row");
    assert_eq!(
        f.values,
        ["me", "box"],
        "typed values come back this session"
    );
    send(
        &mut app,
        [Message::FillCancel, Message::Copy(id), Message::FillRaw],
    );
    assert!(app.fill.is_none());
    assert_eq!(app.vault.commands[0].copies, 2, "as-is is a copy too");
    send(&mut app, [Message::Copy(id), Message::Delete(id)]);
    assert!(app.fill.is_none(), "deleting the command closes the row");
}

#[test]
fn undo_reverses_a_tag_rename_but_not_a_merge() {
    let mut app = app();
    for (text, tags) in [("one", "a"), ("two", "b")] {
        send(
            &mut app,
            [
                command(text),
                Message::TagsChanged(tags.into()),
                Message::Submit,
            ],
        );
    }
    send(
        &mut app,
        [
            Message::TagRenameStart("a".into()),
            Message::TagRenameChanged("c".into()),
            Message::TagRenameSubmit,
        ],
    );
    assert_eq!(app.vault.commands[0].tags, ["c"]);
    send(&mut app, [Message::Undo]);
    assert_eq!(app.vault.commands[0].tags, ["a"], "renamed back");
    assert!(app.undo.is_empty());

    send(
        &mut app,
        [
            Message::TagRenameStart("a".into()),
            Message::TagRenameChanged("b".into()),
            Message::TagRenameSubmit,
        ],
    );
    assert_eq!(app.vault.commands[0].tags, ["b"]);
    assert!(app.undo.is_empty(), "a merge records no undo step");
}

#[test]
fn tag_remove_strips_the_tag_and_undo_restores_it() {
    let mut app = app();
    for text in ["one", "two"] {
        send(
            &mut app,
            [
                command(text),
                Message::TagsChanged("shared".into()),
                Message::Submit,
            ],
        );
    }
    send(
        &mut app,
        [Message::TagRenameStart("shared".into()), Message::TagRemove],
    );
    assert!(app.tag_rename.is_none());
    assert!(app.vault.commands.iter().all(|c| c.tags.is_empty()));
    assert!(app.vault.tag_colour("shared").is_none());
    send(&mut app, [Message::Undo]);
    assert!(app.vault.commands.iter().all(|c| c.tags == ["shared"]));
    assert!(app.vault.tag_colour("shared").is_some());
}

#[test]
fn undo_restores_the_version_before_an_edit() {
    let mut app = app();
    send(&mut app, [command("ls"), Message::Submit]);
    let id = app.vault.commands[0].id;
    send(
        &mut app,
        [
            Message::Edit(id),
            Message::TitleChanged("Listing".into()),
            Message::Submit,
        ],
    );
    assert_eq!(app.vault.commands[0].title, "Listing");
    assert!(matches!(app.undo.last(), Some(Undo::Edited(c)) if c.title == "ls"));
    send(
        &mut app,
        [Message::TogglePin(id), Message::Copy(id), Message::Undo],
    );
    assert_eq!(app.vault.commands.len(), 1);
    assert_eq!(app.vault.commands[0].title, "ls");
    assert_eq!(app.vault.commands[0].id, id, "same command, older content");
    assert!(
        app.vault.commands[0].pinned,
        "a pin set after the edit stays"
    );
    assert_eq!(
        app.vault.commands[0].copies, 1,
        "a copy made after the edit stays"
    );

    send(&mut app, [Message::Edit(id)]);
    assert_eq!(app.selected, Some(id), "Edit highlights its row");
}

#[test]
fn undo_steps_back_through_several_actions() {
    let mut app = app();
    send(&mut app, [command("one"), Message::Submit]);
    app.import_history("two\ntwo\n", "test");
    let (one, two) = (app.vault.commands[0].id, app.vault.commands[1].id);
    send(&mut app, [Message::Delete(one), Message::Delete(two)]);
    assert!(app.vault.commands.is_empty());
    assert_eq!(app.undo.len(), 3);
    send(&mut app, [Message::Undo, Message::Undo]);
    assert_eq!(app.vault.commands.len(), 2, "both deletes undone");
    send(&mut app, [Message::Undo]);
    assert_eq!(app.vault.commands.len(), 1, "then the import");
    assert_eq!(app.vault.commands[0].command_text, "one");
    assert!(app.undo.is_empty());

    for _ in 0..(UNDO_DEPTH + 5) {
        send(&mut app, [command("x"), Message::Submit]);
        let id = app.vault.commands.last().unwrap().id;
        send(&mut app, [Message::Delete(id)]);
    }
    assert_eq!(app.undo.len(), UNDO_DEPTH, "the oldest steps drop off");
}

#[test]
fn undo_removes_an_import_in_one_step() {
    let mut app = app();
    send(&mut app, [command("ls -la"), Message::Submit]);
    app.import_history(
        "ls -la\nls -la\ngit status\ngit status\ncargo build\n",
        "test",
    );
    assert_eq!(
        app.vault.commands.len(),
        2,
        "one new, one already stored, one used once"
    );
    assert!(matches!(app.undo.last(), Some(Undo::Imported(ids)) if ids.len() == 1));
    let imported = app.vault.commands[1].id;
    send(&mut app, [Message::Select(imported), Message::Undo]);
    assert_eq!(app.vault.commands.len(), 1);
    assert_eq!(
        app.vault.commands[0].command_text, "ls -la",
        "the old command stays"
    );
    assert_eq!(app.selected, None);
    assert!(app.undo.is_empty());

    app.import_history("ls -la\nls -la\n", "test");
    assert!(
        app.undo.is_empty(),
        "an import that adds nothing is not an undo step"
    );
}

#[test]
fn sort_orders_by_title_or_by_last_update() {
    let mut app = app();
    for title in ["bravo", "alpha"] {
        send(
            &mut app,
            [
                Message::TitleChanged(title.into()),
                command("c"),
                Message::Submit,
            ],
        );
    }
    let titles =
        |app: &App| -> Vec<String> { app.visible().iter().map(|c| c.title.clone()).collect() };
    assert_eq!(titles(&app), ["alpha", "bravo"]);

    let bravo = app
        .vault
        .commands
        .iter()
        .find(|c| c.title == "bravo")
        .unwrap()
        .id;
    send(
        &mut app,
        [
            Message::Edit(bravo),
            Message::Submit,
            Message::SortChanged(Sort::Recent),
        ],
    );
    assert_eq!(titles(&app), ["bravo", "alpha"]);
    assert_eq!(app.settings.sort, Sort::Recent);

    let alpha = app
        .vault
        .commands
        .iter()
        .find(|c| c.title == "alpha")
        .unwrap()
        .id;
    send(
        &mut app,
        [
            Message::Copy(alpha),
            Message::Copy(alpha),
            Message::SortChanged(Sort::Popular),
        ],
    );
    assert_eq!(titles(&app), ["alpha", "bravo"]);
    assert_eq!(
        app.vault
            .commands
            .iter()
            .find(|c| c.id == alpha)
            .unwrap()
            .copies,
        2
    );

    send(
        &mut app,
        [Message::Copy(bravo), Message::SortChanged(Sort::Copied)],
    );
    assert_eq!(
        titles(&app),
        ["bravo", "alpha"],
        "the last copy comes first"
    );
    send(&mut app, [command("c"), Message::Submit]);
    assert_eq!(
        titles(&app),
        ["bravo", "alpha", "c"],
        "never copied sorts last"
    );
}

#[test]
fn resize_is_kept_in_settings_undoing_the_zoom() {
    let mut app = app();
    send(&mut app, [Message::Resized(Size::new(1000.0, 700.0))]);
    assert_eq!(
        app.settings.window,
        Some(WindowSize {
            width: 1000.0,
            height: 700.0
        })
    );
    send(
        &mut app,
        [
            Message::Zoom(2.0),
            Message::Resized(Size::new(500.0, 350.0)),
        ],
    );
    assert_eq!(
        app.settings.window,
        Some(WindowSize {
            width: 1000.0,
            height: 700.0
        }),
        "the same physical window, stored the way winit will reopen it"
    );
}

#[test]
fn clipboard_is_cleared_only_by_the_latest_copy_while_it_still_holds_it() {
    let mut app = app();
    send(&mut app, [command("secret"), Message::Submit]);
    let id = app.vault.commands[0].id;
    send(&mut app, [Message::Copy(id), Message::Copy(id)]);
    assert_eq!(app.copy_generation, 2, "every copy bumps it");
    let read = |generation: u64, now: Option<&str>| Message::ClipboardRead {
        copied: "secret".into(),
        now: now.map(Into::into),
        generation,
    };
    app.status = "Copied.".into();
    send(&mut app, [read(2, Some("something else"))]);
    assert_eq!(app.status, "Copied.", "another program's text, keep it");
    send(&mut app, [read(2, None)]);
    assert_eq!(app.status, "Copied.", "nothing to clear");
    send(&mut app, [read(1, Some("secret"))]);
    assert_eq!(
        app.status, "Copied.",
        "the first copy's timer is stale, the second copy keeps its full delay"
    );
    send(&mut app, [read(2, Some("secret"))]);
    assert_eq!(app.status, "Clipboard cleared.");
}

#[test]
fn copy_status_names_the_clear_delay_unless_it_is_off() {
    // "Copied" only replaces "Saved" after a real save, hence the scratch vault.
    let dir = std::env::temp_dir().join(format!("commandvault-test-{}", Uuid::new_v4()));
    let mut app = app();
    app.path = Some(dir.join("vault.json"));
    send(&mut app, [command("c"), Message::Submit]);
    let id = app.vault.commands[0].id;
    send(&mut app, [Message::Copy(id)]);
    assert_eq!(app.status, "Copied \"c\", clipboard clears in 30 s.");
    app.settings.clear_after = 0;
    send(&mut app, [Message::Copy(id)]);
    assert_eq!(app.status, "Copied \"c\".");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn f1_reaches_the_app_from_a_focused_input_and_escape_closes_the_keys_first() {
    let pressed = |named: key::Named, status: event::Status| {
        let Message::Key(e) = key(keyboard::Key::Named(named), false) else {
            unreachable!()
        };
        on_event(Event::Keyboard(e), status, window::Id::unique())
    };
    assert!(matches!(
        pressed(key::Named::F1, event::Status::Captured),
        Some(Message::ToggleHelp)
    ));
    assert!(matches!(
        pressed(key::Named::Escape, event::Status::Captured),
        Some(Message::CloseHelp)
    ));
    assert!(matches!(
        pressed(key::Named::Escape, event::Status::Ignored),
        Some(Message::Key(_))
    ));
    assert!(
        pressed(key::Named::Tab, event::Status::Captured).is_none(),
        "a captured key stays with its widget"
    );

    let mut app = app();
    send(
        &mut app,
        [Message::SearchChanged("ls".into()), Message::ToggleHelp],
    );
    assert!(app.help);
    send(
        &mut app,
        [key(keyboard::Key::Named(key::Named::Escape), false)],
    );
    assert!(!app.help);
    assert_eq!(app.search, "ls", "that Escape only closed the keys");
    send(&mut app, [Message::CloseHelp]);
    assert_eq!(
        app.search, "ls",
        "a captured Escape with no panel does nothing"
    );
    send(&mut app, [Message::ToggleHelp, Message::ToggleHelp]);
    assert!(!app.help, "F1 again closes them");

    send(
        &mut app,
        [
            Message::SearchChanged(String::new()),
            command("c"),
            Message::Submit,
            Message::ToggleHelp,
        ],
    );
    send(
        &mut app,
        [key(keyboard::Key::Named(key::Named::ArrowDown), false)],
    );
    assert_eq!(app.selected, None, "the panel is modal, Down stays with it");
    send(
        &mut app,
        [
            key(keyboard::Key::Named(key::Named::Escape), false),
            key(keyboard::Key::Named(key::Named::ArrowDown), false),
        ],
    );
    assert!(!app.help);
    assert!(app.selected.is_some(), "Escape closed it, Down moves again");
}

#[test]
fn hover_follows_the_pointer_over_sidebar_entries() {
    let mut app = app();
    let id = Uuid::new_v4();
    send(&mut app, [Message::HoverCategory(Some(Some(id)))]);
    assert_eq!(app.hovered, Some(Some(id)));
    send(&mut app, [Message::HoverCategory(Some(None))]);
    assert_eq!(app.hovered, Some(None), "the All entry");
    send(&mut app, [Message::HoverCategory(None)]);
    assert_eq!(app.hovered, None, "left the sidebar");
}

#[test]
fn ctrl_shift_b_toggles_compact_rows_and_saves_it() {
    let mut app = app();
    let mut shifted = key(keyboard::Key::Character("B".into()), true);
    if let Message::Key(keyboard::Event::KeyPressed { modifiers, .. }) = &mut shifted {
        *modifiers |= keyboard::Modifiers::SHIFT;
    }
    assert!(!app.settings.compact);
    send(&mut app, [shifted.clone()]);
    assert!(app.settings.compact);
    assert_eq!(app.status, "Compact rows for this session only.");
    assert!(
        app.settings.sidebar,
        "the sidebar toggle is a different key"
    );
    send(&mut app, [shifted]);
    assert!(!app.settings.compact);
    assert_eq!(app.status, "Full rows for this session only.");
}

#[test]
fn a_tick_reloads_the_vault_when_the_file_changed_on_disk() {
    let dir = std::env::temp_dir().join(format!("commandvault-test-{}", Uuid::new_v4()));
    let path = dir.join("vault.json");
    let mut app = app();
    app.path = Some(path.clone());
    send(&mut app, [command("mine"), Message::Submit]);
    send(&mut app, [Message::Tick]);
    assert_eq!(app.status, "Saved.", "nothing changed, nothing said");

    let mut other = Vault::default();
    other.commands.push(Command::new(Draft {
        command_text: "theirs".into(),
        ..Draft::default()
    }));
    std::thread::sleep(std::time::Duration::from_millis(20));
    storage::save(&path, &other).unwrap();
    send(&mut app, [Message::Tick]);
    assert_eq!(app.vault.commands[0].command_text, "theirs");
    assert_eq!(app.status, "Reloaded 1 commands, the file changed on disk.");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "a measurement, run with --ignored and read the time"]
fn search_over_ten_thousand_commands_is_fast_enough() {
    let mut app = app();
    for i in 0..10_000 {
        app.vault.commands.push(Command::new(Draft {
            title: format!("Command number {i}"),
            command_text: format!("echo {i} && ls -la /tmp/{i}"),
            description: "a description with some words in it".into(),
            tags: vec![format!("tag{}", i % 50)],
            ..Draft::default()
        }));
    }
    app.search = "echo 99 tag4".into();
    let start = std::time::Instant::now();
    let n = app.visible().len();
    let took = start.elapsed();
    eprintln!("visible() over 10000 commands, {n} hits: {took:?}");
    assert!(took.as_millis() < 500, "a keystroke must not lag: {took:?}");
}

#[test]
fn ctrl_b_toggles_the_sidebar_and_saves_it() {
    let mut app = app();
    send(&mut app, [command("c"), Message::Submit]);
    assert!(app.settings.sidebar, "shown by default");
    send(&mut app, [key(keyboard::Key::Character("b".into()), true)]);
    assert!(!app.settings.sidebar);
    assert_eq!(app.status, "Sidebar hidden for this session only.");
    assert_eq!(app.selected, None, "Ctrl+B does not touch the highlight");
    send(&mut app, [key(keyboard::Key::Character("b".into()), true)]);
    assert!(app.settings.sidebar);
    assert_eq!(app.status, "Sidebar shown for this session only.");
    send(&mut app, [key(keyboard::Key::Character("b".into()), false)]);
    assert!(app.settings.sidebar, "a plain b is not a toggle");
}

#[test]
fn zoom_keys_step_and_clamp() {
    let mut app = app();
    send(&mut app, [command("c"), Message::Submit]);
    send(
        &mut app,
        [key(keyboard::Key::Named(key::Named::ArrowUp), true)],
    );
    assert_eq!(app.settings.zoom, 1.1);
    assert_eq!(app.status, "Zoom 110% for this session only.");
    assert_eq!(
        app.selected, None,
        "Ctrl+Up zooms, it does not move the highlight"
    );
    send(&mut app, [Message::Zoom(9.0)]);
    assert_eq!(app.settings.zoom, 2.0, "clamped at the top");
    for _ in 0..20 {
        send(
            &mut app,
            [key(keyboard::Key::Named(key::Named::ArrowDown), true)],
        );
    }
    assert_eq!(app.settings.zoom, 0.5, "clamped at the bottom");
    send(
        &mut app,
        [key(keyboard::Key::Named(key::Named::ArrowDown), false)],
    );
    assert_eq!(app.settings.zoom, 0.5, "a plain Down moves the highlight");
    assert!(app.selected.is_some());
}

#[test]
fn undo_puts_a_renamed_or_moved_category_back() {
    let mut app = app();
    let linux = app.vault.add_category("linux".into(), None);
    let other = app.vault.add_category("other".into(), None);
    send(
        &mut app,
        [
            Message::SelectCategory(Some(linux)),
            Message::EditCategory,
            Message::NewCategoryChanged("gnu".into()),
            Message::NewCategoryParent(Choice {
                id: Some(other),
                label: "other".into(),
            }),
            Message::SubmitCategory,
        ],
    );
    let c = app.vault.categories.iter().find(|c| c.id == linux).unwrap();
    assert_eq!((c.name.as_str(), c.parent_id), ("gnu", Some(other)));
    send(&mut app, [Message::Undo]);
    let c = app.vault.categories.iter().find(|c| c.id == linux).unwrap();
    assert_eq!((c.name.as_str(), c.parent_id), ("linux", None));
}

#[test]
fn alt_arrows_walk_the_category_tree() {
    let mut app = app();
    let linux = app.vault.add_category("linux".into(), None);
    let net = app.vault.add_category("network".into(), Some(linux));
    let mut alt_down = key(keyboard::Key::Named(key::Named::ArrowDown), false);
    let mut alt_up = key(keyboard::Key::Named(key::Named::ArrowUp), false);
    for k in [&mut alt_down, &mut alt_up] {
        if let Message::Key(keyboard::Event::KeyPressed { modifiers, .. }) = k {
            *modifiers = keyboard::Modifiers::ALT;
        }
    }
    send(&mut app, [alt_down.clone()]);
    assert_eq!(
        app.selected_category,
        Some(linux),
        "All, then the first category"
    );
    send(&mut app, [alt_down.clone(), alt_down.clone()]);
    assert_eq!(app.selected_category, Some(net), "clamped at the last");
    send(&mut app, [alt_up.clone(), alt_up.clone(), alt_up.clone()]);
    assert_eq!(app.selected_category, None, "back to All and no further");
    assert_eq!(app.selected, None, "the row highlight is untouched");
}

#[test]
fn f2_edits_the_selected_category() {
    let mut app = app();
    let linux = app.vault.add_category("linux".into(), None);
    send(&mut app, [key(keyboard::Key::Named(key::Named::F2), false)]);
    assert_eq!(
        app.editing_category, None,
        "nothing selected, nothing to edit"
    );
    send(
        &mut app,
        [
            Message::SelectCategory(Some(linux)),
            key(keyboard::Key::Named(key::Named::F2), false),
        ],
    );
    assert_eq!(app.editing_category, Some(linux));
    assert_eq!(app.new_category, "linux");
}

#[test]
fn category_actions_report_in_the_status_line() {
    // These statuses only replace "Saved" after a real save, so this test writes
    // to a scratch vault instead of running with no path.
    let dir = std::env::temp_dir().join(format!("commandvault-test-{}", Uuid::new_v4()));
    let mut app = app();
    app.path = Some(dir.join("vault.json"));
    send(
        &mut app,
        [
            Message::NewCategoryChanged("linux".into()),
            Message::SubmitCategory,
        ],
    );
    assert_eq!(app.status, "Added category \"linux\".");
    let linux = app.vault.categories[0].id;
    send(
        &mut app,
        [
            Message::SelectCategory(Some(linux)),
            command("ls"),
            Message::Submit,
            Message::DeleteCategory,
        ],
    );
    assert_eq!(
        app.status,
        "Removed \"linux\". 1 command and any subcategories moved up, Undo puts them back."
    );
    assert_eq!(app.vault.commands[0].category_id, None);
    send(&mut app, [Message::Undo]);
    assert_eq!(app.status, "Restored category \"linux\".");
    assert_eq!(app.vault.categories.len(), 1);
    assert_eq!(app.vault.commands[0].category_id, Some(linux));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_vault_changed_on_disk_is_kept_beside_the_new_save() {
    let dir = std::env::temp_dir().join(format!("commandvault-test-{}", Uuid::new_v4()));
    let path = dir.join("vault.json");
    let mut app = app();
    app.path = Some(path.clone());
    send(&mut app, [command("one"), Message::Submit]);
    assert_eq!(app.status, "Saved.");
    assert!(app.vault_mtime.is_some());

    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&path, r#"{"commands":[]}"#).unwrap();
    send(&mut app, [command("two"), Message::Submit]);
    assert!(app.status.contains("kept as"), "{}", app.status);
    let kept: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bak") && n != "vault.json.bak")
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(
        std::fs::read_to_string(dir.join(&kept[0])).unwrap(),
        r#"{"commands":[]}"#
    );
    let saved = storage::load::<Vault>(&path).unwrap();
    assert_eq!(saved.commands.len(), 2, "our version is what got saved");

    send(&mut app, [command("three"), Message::Submit]);
    assert_eq!(app.status, "Saved.", "our own save is not a change on disk");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reload_reads_the_file_again_and_keeps_a_bad_file_out() {
    let dir = std::env::temp_dir().join(format!("commandvault-test-{}", Uuid::new_v4()));
    let mut app = app();
    send(&mut app, [key(keyboard::Key::Character("r".into()), true)]);
    assert!(app.status.starts_with("No vault file"), "{}", app.status);

    let path = dir.join("vault.json");
    app.path = Some(path.clone());
    send(&mut app, [command("one"), Message::Submit]);
    let mut outside = storage::load::<Vault>(&path).unwrap();
    outside.commands.push(Command::new(Draft {
        title: "two".into(),
        command_text: "two".into(),
        ..Draft::default()
    }));
    storage::save(&path, &outside).unwrap();
    send(&mut app, [Message::Reload]);
    assert_eq!(app.vault.commands.len(), 2);
    assert_eq!(app.status, "Reloaded 2 commands.");

    let linux = app.vault.add_category("linux".into(), None);
    send(&mut app, [Message::SelectCategory(Some(linux))]);
    assert_eq!(app.selected_category, Some(linux));
    send(&mut app, [Message::Reload]);
    assert_eq!(
        app.selected_category, None,
        "the file never had that category"
    );
    assert_eq!(app.settings.category, None);
    assert_eq!(app.visible().len(), 2, "back to the whole list");

    std::fs::write(&path, "not json").unwrap();
    send(&mut app, [Message::Reload]);
    assert_eq!(app.vault.commands.len(), 2, "the loaded vault stays");
    assert!(app.status.starts_with("Could not reload"), "{}", app.status);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn scheme_change_is_kept_and_reported() {
    let mut app = app();
    assert_eq!(app.theme(), None, "default follows the system");
    send(
        &mut app,
        [Message::SchemeChanged(Scheme::Theme(iced::Theme::Dark))],
    );
    assert_eq!(app.settings.scheme, Scheme::Theme(iced::Theme::Dark));
    assert_eq!(app.theme(), Some(iced::Theme::Dark));
    assert_eq!(app.status, "Dark colours for this session only.");
}

#[test]
fn pinned_commands_sort_first_and_ctrl_p_toggles_the_highlight() {
    let mut app = app();
    for title in ["alpha", "bravo", "charlie"] {
        send(
            &mut app,
            [
                Message::TitleChanged(title.into()),
                command("c"),
                Message::Submit,
            ],
        );
    }
    let titles =
        |app: &App| -> Vec<String> { app.visible().iter().map(|c| c.title.clone()).collect() };
    let charlie = app.vault.commands[2].id;
    send(&mut app, [Message::TogglePin(charlie)]);
    assert_eq!(titles(&app), ["charlie", "alpha", "bravo"]);
    send(
        &mut app,
        [
            Message::Select(charlie),
            key(keyboard::Key::Character("p".into()), true),
        ],
    );
    assert_eq!(titles(&app), ["alpha", "bravo", "charlie"], "Ctrl+P unpins");
    assert!(!app.vault.commands[2].pinned);
}

#[test]
fn export_reports_the_visible_count() {
    let mut app = app();
    for title in ["one", "two"] {
        send(
            &mut app,
            [
                Message::TitleChanged(title.into()),
                command("c"),
                Message::Submit,
            ],
        );
    }
    send(
        &mut app,
        [Message::SearchChanged("two".into()), Message::Export],
    );
    assert!(
        app.status.starts_with("Copied Markdown for 1 commands"),
        "{}",
        app.status
    );
}

#[test]
fn duplicate_text_is_reported_except_for_the_command_being_edited() {
    let mut app = app();
    send(
        &mut app,
        [
            Message::TitleChanged("first".into()),
            command("ls"),
            Message::Submit,
        ],
    );
    let ls = Draft {
        command_text: "ls".into(),
        ..Draft::default()
    };
    assert_eq!(app.duplicate_of(&ls).as_deref(), Some("first"));
    let id = app.vault.commands[0].id;
    send(&mut app, [Message::Edit(id)]);
    assert_eq!(
        app.duplicate_of(&ls),
        None,
        "editing itself is not a duplicate"
    );
}

#[test]
fn search_matches_the_category_path() {
    let mut app = app();
    let linux = app.vault.add_category("linux".into(), None);
    send(
        &mut app,
        [
            Message::SelectCategory(Some(linux)),
            Message::TitleChanged("t".into()),
            command("c"),
            Message::Submit,
            Message::SelectCategory(None),
            Message::SearchChanged("LINUX".into()),
        ],
    );
    assert_eq!(app.visible().len(), 1);
    send(&mut app, [Message::SearchChanged("linux c".into())]);
    assert_eq!(app.visible().len(), 1, "path and command text together");
    send(&mut app, [Message::SearchChanged("windows".into())]);
    assert_eq!(app.visible().len(), 0);
}

#[test]
fn right_click_cycles_a_tag_colour() {
    let mut app = app();
    send(
        &mut app,
        [
            Message::TitleChanged("t".into()),
            command("c"),
            Message::TagsChanged("x".into()),
            Message::Submit,
        ],
    );
    let before = app.vault.tag_colour("x").unwrap().to_owned();
    send(&mut app, [Message::CycleTagColour("x".into())]);
    assert_ne!(app.vault.tag_colour("x").unwrap(), before);
}

#[test]
fn tag_rename_flow_through_messages() {
    let mut app = app();
    send(
        &mut app,
        [
            Message::TitleChanged("t".into()),
            command("c"),
            Message::TagsChanged("fs".into()),
            Message::Submit,
            Message::TagRenameStart("fs".into()),
            Message::TagRenameChanged("files".into()),
            Message::TagRenameSubmit,
        ],
    );
    assert_eq!(app.vault.commands[0].tags, ["files"]);
    assert!(app.tag_rename.is_none());
    send(
        &mut app,
        [Message::TagRenameStart("files".into()), Message::Reset],
    );
    assert!(app.tag_rename.is_none(), "Escape cancels a rename");
}
