//! Import of frequently used commands from a shell history file.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use uuid::Uuid;

use crate::model::{Command, Draft, Vault, title_for};

/// `$HISTFILE`, else the zsh, bash or fish history under the home directory,
/// whichever exists first.
pub fn history_path() -> Option<PathBuf> {
    if let Some(p) = env::var_os("HISTFILE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    let home = env::home_dir()?;
    [
        ".zsh_history",
        ".bash_history",
        ".local/share/fish/fish_history",
    ]
    .iter()
    .map(|name| home.join(name))
    .find(|p| p.is_file())
}

/// History files can hold bytes that are not UTF-8. Lossy is fine for a picker.
pub fn read(path: &Path) -> io::Result<String> {
    fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Commands used at least `min_count` times, most frequent first, at most `limit`.
/// Zsh extended lines `: <epoch>:<duration>;<command>` and fish records `- cmd: <command>`
/// are unwrapped, the `when:` and `paths:` lines of a fish record and bash's `#<epoch>`
/// timestamp lines are dropped, other lines are taken as they are. Lines under three
/// characters are noise and skipped.
// Known limit: a multi-line zsh entry counts as separate lines. Rare in a vault of
// one-liners, join on a trailing backslash if it shows up.
pub fn frequent(text: &str, min_count: usize, limit: usize) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        let command = if let Some(rest) = line.strip_prefix(": ") {
            rest.split_once(';').map_or(rest, |(_, c)| c).trim()
        } else if let Some(rest) = line.strip_prefix("- cmd: ") {
            rest.trim()
        } else if line.starts_with("when: ") || line.starts_with("paths:") || line.starts_with("- ")
        {
            continue;
        } else {
            line
        };
        // Bash writes "#<epoch>" comment lines when HISTTIMEFORMAT is set.
        let timestamp = command
            .strip_prefix('#')
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
        if command.chars().count() < 3 || timestamp {
            continue;
        }
        *counts.entry(command).or_default() += 1;
    }
    let mut ranked: Vec<(String, usize)> = counts
        .into_iter()
        .filter(|(_, n)| *n >= min_count)
        .map(|(c, n)| (c.to_owned(), n))
        .collect();
    ranked.sort_by_cached_key(|(c, n)| (std::cmp::Reverse(*n), c.clone()));
    ranked.truncate(limit);
    ranked
}

/// Adds the ranked commands that are not in the vault yet. Returns the ids added, so
/// the caller can offer them as one undo step.
pub fn import(
    vault: &mut Vault,
    ranked: &[(String, usize)],
    category_id: Option<Uuid>,
) -> Vec<Uuid> {
    let mut added = Vec::new();
    for (text, count) in ranked {
        if vault.commands.iter().any(|c| &c.command_text == text) {
            continue;
        }
        let command = Command::new(Draft {
            title: title_for(text),
            description: format!("Imported from shell history, used {count} times."),
            command_text: text.clone(),
            tags: vec!["history".into()],
            category_id,
        });
        added.push(command.id);
        vault.commands.push(command);
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
: 1700000000:0;git status
: 1700000001:0;ls
: 1700000002:0;git status
cargo build
cargo build
cargo build
: 1700000003:5;du -sh /* 2>/dev/null | sort -h
ls
#1700000004
#1700000004
- cmd: git status
  when: 1700000005
- cmd: fish_config
  when: 1700000006
  paths:
    - ~/.config/fish
- cmd: fish_config
  when: 1700000007
";

    #[test]
    fn frequent_unwraps_zsh_counts_and_ranks() {
        let ranked = frequent(SAMPLE, 2, 10);
        assert_eq!(
            ranked,
            [
                ("cargo build".to_owned(), 3),
                ("git status".to_owned(), 3),
                ("fish_config".to_owned(), 2)
            ],
            "zsh, bash and fish lines count together"
        );
        assert!(
            frequent(SAMPLE, 1, 20)
                .iter()
                .all(|(c, _)| !c.starts_with("when")
                    && !c.starts_with("paths")
                    && !c.starts_with('~')),
            "fish record fields are not commands"
        );
        assert_eq!(frequent(SAMPLE, 1, 1).len(), 1, "limit applies");
        assert!(
            frequent(SAMPLE, 1, 10).iter().all(|(c, _)| c != "ls"),
            "two-character noise skipped"
        );
        assert!(
            frequent(SAMPLE, 1, 10)
                .iter()
                .all(|(c, _)| !c.starts_with('#')),
            "bash timestamp lines skipped"
        );
    }

    #[test]
    fn import_skips_commands_already_in_the_vault_and_titles_them() {
        let mut v = Vault::default();
        v.commands.push(Command::new(Draft {
            title: "Build".into(),
            command_text: "cargo build".into(),
            ..Draft::default()
        }));
        let ranked = frequent(SAMPLE, 2, 10);
        let added = import(&mut v, &ranked, None);
        assert_eq!(added, [v.commands[1].id, v.commands[2].id]);
        assert!(
            import(&mut v, &ranked, None).is_empty(),
            "second import adds nothing"
        );
        let added = &v.commands[1];
        assert_eq!(added.title, "git status");
        assert_eq!(added.tags, ["history"]);
        assert!(added.description.contains("used 3 times"));
        assert_eq!(title_for(&"x".repeat(100)).chars().count(), 60);
    }
}
