use std::fmt::Write;
use std::io;

use chrono::SecondsFormat;
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

/// A vault document holding only the given commands, the categories above them and the
/// colours of their tags, so it loads as a vault file of its own. The lossless export.
pub fn json(vault: &Vault, commands: &[&Command]) -> io::Result<String> {
    let subset = Vault {
        commands: commands.iter().map(|&c| c.clone()).collect(),
        categories: vault
            .categories
            .iter()
            .filter(|cat| {
                commands
                    .iter()
                    .any(|c| vault.is_within(c.category_id, cat.id))
            })
            .cloned()
            .collect(),
        tags: vault
            .tags
            .iter()
            .filter(|t| commands.iter().any(|c| c.tags.contains(&t.name)))
            .cloned()
            .collect(),
    };
    serde_json::to_string_pretty(&subset).map_err(io::Error::other)
}

/// A spreadsheet of the given commands in their given order, a header row first and
/// CRLF line ends as RFC 4180 has them.
pub fn csv(vault: &Vault, commands: &[&Command]) -> String {
    let mut out =
        String::from("title,command,description,category,tags,pinned,copies,created,updated\r\n");
    for c in commands {
        let (path, tags) = (vault.path_of(c.category_id), c.tags.join(", "));
        let (pinned, copies) = (c.pinned.to_string(), c.copies.to_string());
        let [created, updated] =
            [c.created_at, c.updated_at].map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true));
        let row = [
            &c.title,
            &c.command_text,
            &c.description,
            &path,
            &tags,
            &pinned,
            &copies,
            &created,
            &updated,
        ]
        .map(|s| cell(s));
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    out
}

/// Quotes a CSV field when it needs it. A spreadsheet runs a cell starting with `=`,
/// `+`, `-` or `@` as a formula, and a command can start with any of them, so such a
/// cell gets a leading `'`, the usual guard. The JSON export keeps the text exact.
fn cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_owned()
    };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
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
    fn json_keeps_the_categories_above_and_the_tags_of_the_given_commands() {
        let mut v = Vault::default();
        let linux = v.add_category("linux".into(), None);
        let ssh = v.add_category("ssh".into(), Some(linux));
        v.add_category("windows".into(), None);
        v.commands
            .push(command("Tunnel", "ssh -L 80:x:80 h", Some(ssh), &["net"]));
        v.commands.push(command("Other", "ls", None, &["files"]));
        v.ensure_tags();
        let only_tunnel = [&v.commands[0]];

        let back: Vault = serde_json::from_str(&json(&v, &only_tunnel).unwrap()).unwrap();

        assert_eq!(back.commands.len(), 1);
        assert_eq!(back.path_of(back.commands[0].category_id), "linux / ssh");
        assert_eq!(back.categories.len(), 2, "windows left out");
        let net: Vec<_> = v.tags.iter().filter(|t| t.name == "net").cloned().collect();
        assert_eq!(back.tags, net, "only the net colour");
    }

    #[test]
    fn csv_quotes_fields_and_guards_formula_cells() {
        let mut v = Vault::default();
        let linux = v.add_category("linux".into(), None);
        let mut c = command("Say \"hi\"", "=cmd|' /C calc'!A0", Some(linux), &["a", "b"]);
        c.description = "two\nlines".into();
        v.commands.push(c);
        let all: Vec<&Command> = v.commands.iter().collect();

        let out = csv(&v, &all);

        assert!(out.starts_with("title,command,description,category,tags,"));
        assert!(out.contains(
            "\r\n\"Say \"\"hi\"\"\",'=cmd|' /C calc'!A0,\"two\nlines\",linux,\"a, b\",false,0,"
        ));
        assert!(out.ends_with("Z\r\n"));
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
