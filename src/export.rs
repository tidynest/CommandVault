use std::fmt::Write;

use uuid::Uuid;

use crate::model::{Command, Vault};

/// Markdown for the given commands, grouped by the category tree in display order.
/// Categories without any of these commands are left out. Uncategorised ones come last.
/// Command headings sit one level below their category heading.
pub fn markdown(vault: &Vault, commands: &[&Command]) -> String {
    let mut out = String::from("# CommandVault\n");
    for (depth, category) in vault.tree() {
        let mine: Vec<&Command> = commands
            .iter()
            .copied()
            .filter(|c| c.category_id == Some(category.id))
            .collect();
        let any_below = commands
            .iter()
            .any(|c| vault.is_within(c.category_id, category.id));
        if !any_below {
            continue;
        }
        let _ = write!(out, "\n{} {}\n", heading(depth + 2), category.name);
        for c in mine {
            push_command(&mut out, c, depth + 3);
        }
    }
    let loose: Vec<&Command> = commands
        .iter()
        .copied()
        .filter(|c| c.category_id.is_none() || !known(vault, c.category_id))
        .collect();
    if !loose.is_empty() {
        out.push_str("\n## Uncategorised\n");
        for c in loose {
            push_command(&mut out, c, 3);
        }
    }
    out
}

/// A shell script of the given commands in their given order, each under a comment
/// with its title, description, category path and tags. Placeholders stay as written.
pub fn shell(vault: &Vault, commands: &[&Command]) -> String {
    let n = commands.len();
    let mut out = format!(
        "#!/usr/bin/env bash\n# CommandVault export, {n} command{}\n",
        if n == 1 { "" } else { "s" }
    );
    for c in commands {
        let _ = write!(out, "\n# {}\n", c.title);
        for line in c.description.lines() {
            let _ = writeln!(out, "# {line}");
        }
        let path = vault.path_of(c.category_id);
        let meta: Vec<String> = [
            Some(path).filter(|p| !p.is_empty()),
            (!c.tags.is_empty()).then(|| format!("tags: {}", c.tags.join(", "))),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !meta.is_empty() {
            let _ = writeln!(out, "# {}", meta.join(", "));
        }
        let _ = writeln!(out, "{}", c.command_text);
    }
    out
}

fn known(vault: &Vault, id: Option<Uuid>) -> bool {
    id.is_some_and(|id| vault.categories.iter().any(|c| c.id == id))
}

fn heading(level: usize) -> String {
    "#".repeat(level.min(6))
}

fn push_command(out: &mut String, c: &Command, level: usize) {
    // A command that itself contains a triple backtick needs a longer fence.
    let fence = if c.command_text.contains("```") {
        "````"
    } else {
        "```"
    };
    let _ = write!(out, "\n{} {}\n", heading(level), c.title);
    if !c.description.is_empty() {
        let _ = write!(out, "\n{}\n", c.description);
    }
    let _ = write!(out, "\n{fence}sh\n{}\n{fence}\n", c.command_text);
    if !c.tags.is_empty() {
        let _ = write!(out, "\nTags: {}\n", c.tags.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Draft;

    fn command(title: &str, text: &str, category_id: Option<Uuid>, tags: &[&str]) -> Command {
        Command::new(Draft {
            title: title.into(),
            command_text: text.into(),
            category_id,
            tags: tags.iter().map(ToString::to_string).collect(),
            ..Draft::default()
        })
    }

    #[test]
    fn groups_by_tree_skips_empty_categories_and_fences_backticks() {
        let mut v = Vault::default();
        let linux = v.add_category("linux".into(), None);
        let ssh = v.add_category("ssh".into(), Some(linux));
        v.add_category("windows".into(), None);
        v.commands
            .push(command("Tunnel", "ssh -L 80:x:80 h", Some(ssh), &["net"]));
        v.commands.push(command("Loose", "echo ```", None, &[]));
        let all: Vec<&Command> = v.commands.iter().collect();

        let md = markdown(&v, &all);

        let expected = "# CommandVault\n\n## linux\n\n### ssh\n\n#### Tunnel\n\n```sh\nssh -L 80:x:80 h\n```\n\nTags: net\n\n## Uncategorised\n\n### Loose\n\n````sh\necho ```\n````\n";
        assert_eq!(md, expected);
        assert!(!md.contains("windows"), "empty category skipped");
    }

    #[test]
    fn shell_lists_commands_under_comments_in_the_given_order() {
        let mut v = Vault::default();
        let linux = v.add_category("linux".into(), None);
        let mut tunnel = command("Tunnel", "ssh -L 80:x:80 h", Some(linux), &["net"]);
        tunnel.description = "Two\nlines".into();
        v.commands.push(tunnel);
        v.commands
            .push(command("Loose", "echo one\necho two", None, &[]));
        let all: Vec<&Command> = v.commands.iter().collect();

        let sh = shell(&v, &all);

        let expected = "#!/usr/bin/env bash\n# CommandVault export, 2 commands\n\n# Tunnel\n# Two\n# lines\n# linux, tags: net\nssh -L 80:x:80 h\n\n# Loose\necho one\necho two\n";
        assert_eq!(sh, expected);
    }

    #[test]
    fn exports_only_the_given_commands() {
        let mut v = Vault::default();
        v.commands.push(command("A", "a", None, &[]));
        v.commands.push(command("B", "b", None, &[]));
        let only_b = [&v.commands[1]];
        let md = markdown(&v, &only_b);
        assert!(md.contains("### B") && !md.contains("### A"));
    }
}
