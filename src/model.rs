use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One stored shell command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub command_text: String,
    /// Tag names. Colours live in `Vault::tags`, keyed by name, so this stays plain.
    pub tags: Vec<String>,
    /// `None` is "no category". Vault files written before categories load as `None`.
    #[serde(default)]
    pub category_id: Option<Uuid>,
    /// Times copied to the clipboard, for the "most copied" sort.
    #[serde(default)]
    pub copies: u32,
    /// Sorted first under every order.
    #[serde(default)]
    pub pinned: bool,
    /// When the clipboard last received this command, for the "recently copied" order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_copied: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A node in the category tree. `parent_id` of `None` is top level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
}

/// A tag's colour, keyed by name. Created on first use with a palette colour, so a
/// hand-edited file can change it later and the choice sticks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    /// `#rrggbb`.
    pub colour: String,
}

/// Muted colours that read well under white text.
pub const PALETTE: [&str; 8] = [
    "#5b6abf", "#2e7d6b", "#a4552d", "#7a4b9c", "#3b7dbf", "#8a6d1f", "#b03a5b", "#4c7f2f",
];

/// FNV-1a, folded before the modulo so short names spread across the palette.
/// Stable per name across runs and Rust versions, unlike the std hasher.
fn palette_colour(name: &str) -> &'static str {
    let hash = name.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    PALETTE[(hash ^ (hash >> 16)) as usize % PALETTE.len()]
}

/// The editable fields of a command, already trimmed and parsed.
#[derive(Debug, Clone, Default)]
pub struct Draft {
    pub title: String,
    pub description: String,
    pub command_text: String,
    pub tags: Vec<String>,
    pub category_id: Option<Uuid>,
}

impl Draft {
    /// Only the command is required. The form fills an empty title with `title_for`.
    pub const fn is_valid(&self) -> bool {
        !self.command_text.is_empty()
    }
}

/// Search terms, lowercased: whitespace-separated words, and anything between double
/// quotes kept together. An unclosed quote runs to the end.
pub fn terms(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = query.trim_start();
    while !rest.is_empty() {
        let (term, after) = match rest.strip_prefix('"') {
            Some(quoted) => match quoted.find('"') {
                Some(end) => (&quoted[..end], &quoted[end + 1..]),
                None => (quoted, ""),
            },
            None => {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                (&rest[..end], &rest[end..])
            }
        };
        if !term.trim().is_empty() {
            out.push(term.trim().to_lowercase());
        }
        rest = after.trim_start();
    }
    out
}

/// "just now", "n minutes ago", "n hours ago" or "n days ago", the largest unit that
/// fits. A time in the future, from a clock that moved, reads as just now.
pub fn ago(t: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - t).num_seconds().max(0);
    let (n, unit) = match secs {
        s if s < 60 => return "just now".into(),
        s if s < 3600 => (s / 60, "minute"),
        s if s < 86_400 => (s / 3600, "hour"),
        s => (s / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// Whether `name` is one of `list`, whole and case-insensitively. Empty names never are.
fn names_one<'a>(mut list: impl Iterator<Item = &'a str>, name: &str) -> bool {
    !name.is_empty() && list.any(|s| s.eq_ignore_ascii_case(name))
}

/// Names inside `{{ }}` markers with their defaults, `{{port=22}}` giving "22" and a bare
/// `{{host}}` an empty string. Trimmed, unique by name, in order of first appearance,
/// the first default wins.
pub fn placeholders(text: &str) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = Vec::new();
    let mut rest = text;
    while let Some((name, default, after)) = next_marker(rest) {
        if !name.is_empty() && !names.iter().any(|(n, _)| n == name) {
            names.push((name.to_owned(), default.to_owned()));
        }
        rest = after;
    }
    names
}

/// The command with each `{{name}}` or `{{name=default}}` replaced by the value given
/// for that name. A name without a value, or a marker that never closes, stays as written.
pub fn fill(text: &str, values: &[(String, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((name, _, after)) = next_marker(rest) {
        let marker_len = rest.len() - after.len();
        let start = rest.find("{{").unwrap_or(0);
        out.push_str(&rest[..start]);
        match values.iter().find(|(n, _)| n == name) {
            Some((_, value)) => out.push_str(value),
            None => out.push_str(&rest[start..marker_len]),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The next marker in `text`: its trimmed name, its default or "", and the text after.
fn next_marker(text: &str) -> Option<(&str, &str, &str)> {
    let start = text.find("{{")?;
    let inner = &text[start + 2..];
    let end = inner.find("}}")?;
    let (name, default) = inner[..end].split_once('=').unwrap_or((&inner[..end], ""));
    Some((name.trim(), default.trim(), &inner[end + 2..]))
}

/// A title made from a command: its first line, cut to 60 characters on a char boundary.
pub fn title_for(command: &str) -> String {
    command
        .lines()
        .next()
        .unwrap_or(command)
        .chars()
        .take(60)
        .collect()
}

impl Command {
    pub fn new(draft: Draft) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: draft.title,
            description: draft.description,
            command_text: draft.command_text,
            tags: draft.tags,
            category_id: draft.category_id,
            copies: 0,
            pinned: false,
            last_copied: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Replaces the editable fields and stamps `updated_at`.
    pub fn apply(&mut self, draft: Draft) {
        self.title = draft.title;
        self.description = draft.description;
        self.command_text = draft.command_text;
        self.tags = draft.tags;
        self.category_id = draft.category_id;
        self.updated_at = Utc::now();
    }

    /// Every term of the query must appear, case-insensitively, in the title,
    /// description, command text, a tag or the given category path. Terms may hit
    /// different fields, so `docker rm` finds a docker title with rm in the text, while
    /// `"docker rm"` in quotes has to appear as written. `tag:name` needs a tag of
    /// exactly that name and `cat:name` a category of that name on the path. An empty
    /// query matches everything.
    // Known limit: lowercases every field per keystroke. 1.8 ms for 10k commands in release
    // on 2026-09-12, the ignored test in app/tests.rs measures it. An index past ~100k.
    pub fn matches(&self, query: &str, path: &str) -> bool {
        let fields: Vec<String> = [&self.title, &self.description, &self.command_text]
            .into_iter()
            .chain(&self.tags)
            .map(|s| s.to_lowercase())
            .chain(std::iter::once(path.to_lowercase()))
            .collect();
        terms(query).iter().all(|term| {
            if let Some(name) = term.strip_prefix("tag:") {
                names_one(self.tags.iter().map(String::as_str), name)
            } else if let Some(name) = term.strip_prefix("cat:") {
                names_one(path.split(" / "), name)
            } else {
                fields.iter().any(|f| f.contains(term.as_str()))
            }
        })
    }
}

/// Everything persisted, serialised as one JSON document.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Vault {
    pub commands: Vec<Command>,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

/// Guards the ancestor walks against a cycle in a hand-edited file.
const MAX_DEPTH: usize = 64;

impl Vault {
    /// Gives every tag name used by a command a registry entry. Existing entries keep
    /// their colour, so run this after any edit and after loading an older file.
    pub fn ensure_tags(&mut self) {
        let mut names: Vec<&str> = self
            .commands
            .iter()
            .flat_map(|c| c.tags.iter().map(String::as_str))
            .collect();
        names.sort_unstable();
        names.dedup();
        let missing: Vec<Tag> = names
            .into_iter()
            .filter(|n| !self.tags.iter().any(|t| t.name == *n))
            .map(|n| Tag {
                name: n.to_owned(),
                colour: palette_colour(n).to_owned(),
            })
            .collect();
        self.tags.extend(missing);
    }

    /// Moves a tag to the next palette colour. A colour outside the palette, set by
    /// hand in the file, goes to the first one. False when the tag is unknown.
    pub fn cycle_tag_colour(&mut self, name: &str) -> bool {
        let Some(tag) = self.tags.iter_mut().find(|t| t.name == name) else {
            return false;
        };
        let next = PALETTE
            .iter()
            .position(|p| *p == tag.colour)
            .map_or(0, |i| (i + 1) % PALETTE.len());
        tag.colour = PALETTE[next].to_owned();
        true
    }

    /// Renames a tag on every command and in the registry. Renaming onto an existing
    /// tag merges the two. False for an empty or unchanged name, a name with a comma,
    /// which the form uses as the tag separator, or an unknown tag.
    pub fn rename_tag(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty()
            || new == old
            || new.contains(',')
            || !self.tags.iter().any(|t| t.name == old)
        {
            return false;
        }
        if self.tags.iter().any(|t| t.name == new) {
            self.tags.retain(|t| t.name != old);
        } else if let Some(t) = self.tags.iter_mut().find(|t| t.name == old) {
            t.name = new.to_owned();
        }
        for c in &mut self.commands {
            if c.tags.iter().any(|t| t == old) {
                c.tags.retain(|t| t != old && t != new);
                c.tags.push(new.to_owned());
            }
        }
        true
    }

    /// Registry tags in use, by name, each with how many commands carry it. A tag
    /// that no command uses any more stays in the registry for its colour but is
    /// left out here.
    pub fn tag_usage(&self) -> Vec<(&Tag, usize)> {
        let mut used: Vec<(&Tag, usize)> = self
            .tags
            .iter()
            .map(|t| {
                let n = self
                    .commands
                    .iter()
                    .filter(|c| c.tags.contains(&t.name))
                    .count();
                (t, n)
            })
            .filter(|(_, n)| *n > 0)
            .collect();
        used.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        used
    }

    /// Removes a tag from the registry and from every command. Returns the registry
    /// entry and the ids of the commands that carried it, for undo. `None` if unknown.
    pub fn remove_tag(&mut self, name: &str) -> Option<(Tag, Vec<Uuid>)> {
        let pos = self.tags.iter().position(|t| t.name == name)?;
        let tag = self.tags.remove(pos);
        let mut ids = Vec::new();
        for c in &mut self.commands {
            if c.tags.iter().any(|t| t == name) {
                c.tags.retain(|t| t != name);
                ids.push(c.id);
            }
        }
        Some((tag, ids))
    }

    pub fn tag_colour(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.colour.as_str())
    }

    pub fn add_category(&mut self, name: String, parent_id: Option<Uuid>) -> Uuid {
        let id = Uuid::new_v4();
        self.categories.push(Category {
            id,
            name,
            parent_id,
        });
        id
    }

    /// Removes a category. Its child categories and its commands move to its parent.
    pub fn remove_category(&mut self, id: Uuid) {
        let Some(pos) = self.categories.iter().position(|c| c.id == id) else {
            return;
        };
        let parent = self.categories.remove(pos).parent_id;
        for c in &mut self.categories {
            if c.parent_id == Some(id) {
                c.parent_id = parent;
            }
        }
        for c in &mut self.commands {
            if c.category_id == Some(id) {
                c.category_id = parent;
            }
        }
    }

    /// Renames and moves a category. Refuses a parent that is the category itself or
    /// one of its descendants, which would cut the subtree off the tree.
    pub fn update_category(&mut self, id: Uuid, name: String, parent_id: Option<Uuid>) -> bool {
        if parent_id.is_some_and(|p| self.is_within(Some(p), id)) {
            return false;
        }
        let Some(c) = self.categories.iter_mut().find(|c| c.id == id) else {
            return false;
        };
        c.name = name;
        c.parent_id = parent_id;
        true
    }

    /// "linux / network / ssh" for a category, empty for `None` or an unknown id.
    pub fn path_of(&self, category_id: Option<Uuid>) -> String {
        let mut names = Vec::new();
        let mut current = category_id;
        for _ in 0..MAX_DEPTH {
            let Some(c) = current.and_then(|id| self.categories.iter().find(|c| c.id == id)) else {
                break;
            };
            names.push(c.name.as_str());
            current = c.parent_id;
        }
        names.reverse();
        names.join(" / ")
    }

    /// Commands in a category's subtree, or every command for `None`.
    pub fn count_within(&self, category: Option<Uuid>) -> usize {
        self.commands
            .iter()
            .filter(|c| category.is_none_or(|a| self.is_within(c.category_id, a)))
            .count()
    }

    pub fn parent_of(&self, id: Uuid) -> Option<Uuid> {
        self.categories.iter().find(|c| c.id == id)?.parent_id
    }

    /// True when `category` is `ancestor` itself or sits anywhere below it.
    pub fn is_within(&self, category: Option<Uuid>, ancestor: Uuid) -> bool {
        let mut current = category;
        for _ in 0..MAX_DEPTH {
            match current {
                Some(id) if id == ancestor => return true,
                Some(id) => current = self.parent_of(id),
                None => return false,
            }
        }
        false
    }

    /// Depth-first order, siblings sorted by name, each with its depth for indentation.
    // Known limit: rescans the list per level, fine below a few hundred categories.
    pub fn tree(&self) -> Vec<(usize, &Category)> {
        let mut out = Vec::with_capacity(self.categories.len());
        self.push_children(None, 0, &mut out);
        out
    }

    fn push_children<'a>(
        &'a self,
        parent: Option<Uuid>,
        depth: usize,
        out: &mut Vec<(usize, &'a Category)>,
    ) {
        let mut children: Vec<&Category> = self
            .categories
            .iter()
            .filter(|c| c.parent_id == parent)
            .collect();
        children.sort_by(|a, b| a.name.cmp(&b.name));
        for child in children {
            out.push((depth, child));
            if depth < MAX_DEPTH {
                self.push_children(Some(child.id), depth + 1, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(title: &str, command_text: &str) -> Draft {
        Draft {
            title: title.into(),
            description: "Long form".into(),
            command_text: command_text.into(),
            tags: vec!["fs".into()],
            category_id: None,
        }
    }

    /// linux > network > ssh, plus a top-level "windows" that sorts after "linux".
    fn vault_with_tree() -> (Vault, Uuid, Uuid, Uuid) {
        let mut v = Vault::default();
        let linux = v.add_category("linux".into(), None);
        let network = v.add_category("network".into(), Some(linux));
        let ssh = v.add_category("ssh".into(), Some(network));
        v.add_category("windows".into(), None);
        (v, linux, network, ssh)
    }

    #[test]
    fn matches_every_word_across_fields_and_path() {
        let c = Command::new(draft("List files", "ls -la"));
        assert!(c.matches("", ""));
        assert!(c.matches("LIST", ""));
        assert!(c.matches("long", ""));
        assert!(c.matches("-la", ""));
        assert!(c.matches("FS", ""));
        assert!(!c.matches("grep", ""));
        assert!(c.matches("list -la", ""), "words may hit different fields");
        assert!(c.matches("  files   FS ", ""), "extra spaces are ignored");
        assert!(
            !c.matches("list grep", ""),
            "one missing word fails the match"
        );
        assert!(
            c.matches("linux ls", "linux / shell"),
            "the path counts as a field"
        );
        assert!(c.matches("\"ls -la\"", ""), "a quoted phrase as written");
        assert!(
            !c.matches("\"la ls\"", ""),
            "a quoted phrase is not two words"
        );
        assert!(c.matches("\"list files\" -la", ""), "a phrase and a word");
        assert_eq!(terms("  a \"B c\"  d \"open"), ["a", "b c", "d", "open"]);
        assert!(terms("\"\" \" \"").is_empty(), "empty quotes are nothing");
    }

    #[test]
    fn ago_rounds_to_the_largest_unit() {
        let now = Utc::now();
        let back = |secs: i64| now - chrono::Duration::seconds(secs);
        assert_eq!(ago(back(5), now), "just now");
        assert_eq!(ago(back(59), now), "just now");
        assert_eq!(ago(back(60), now), "1 minute ago");
        assert_eq!(ago(back(150), now), "2 minutes ago");
        assert_eq!(ago(back(3600), now), "1 hour ago");
        assert_eq!(ago(back(7 * 3600), now), "7 hours ago");
        assert_eq!(ago(back(86_400), now), "1 day ago");
        assert_eq!(ago(back(40 * 86_400), now), "40 days ago");
        assert_eq!(
            ago(now + chrono::Duration::seconds(30), now),
            "just now",
            "a clock ahead"
        );
    }

    #[test]
    fn tag_and_cat_prefixes_match_only_that_field() {
        let c = Command::new(draft("Docker prune", "docker system prune"));
        assert!(c.matches("tag:fs", ""), "the tag itself");
        assert!(
            !c.matches("tag:docker", ""),
            "a word in the text is not a tag"
        );
        assert!(!c.matches("tag:f", ""), "whole names only");
        assert!(c.matches("TAG:FS", ""), "case does not matter");
        assert!(c.matches("cat:linux", "linux / shell"), "a path segment");
        assert!(c.matches("cat:shell", "linux / shell"), "any segment");
        assert!(
            !c.matches("cat:lin", "linux / shell"),
            "whole segments only"
        );
        assert!(!c.matches("cat:linux", ""), "no path, no match");
        assert!(c.matches("tag:fs prune", ""), "mixes with plain words");
        assert!(c.matches("\"tag:fs\"", ""), "quoted keeps the prefix");
        assert!(!c.matches("tag:", ""), "an empty value matches nothing");
    }

    #[test]
    fn count_within_follows_the_subtree() {
        let (mut v, linux, network, ssh) = vault_with_tree();
        for cat in [Some(linux), Some(ssh), None] {
            let mut c = Command::new(draft("t", "c"));
            c.category_id = cat;
            v.commands.push(c);
        }
        assert_eq!(v.count_within(None), 3);
        assert_eq!(v.count_within(Some(linux)), 2);
        assert_eq!(v.count_within(Some(network)), 1);
        assert_eq!(v.count_within(Some(ssh)), 1);
    }

    #[test]
    fn placeholders_are_found_once_each_and_filled() {
        let text =
            "ssh {{ user }}@{{host}} -p {{ port = 22 }} # {{user=root}} again, {{}} and {{open";
        let markers = placeholders(text);
        let found: Vec<(&str, &str)> = markers
            .iter()
            .map(|(n, d)| (n.as_str(), d.as_str()))
            .collect();
        assert_eq!(
            found,
            [("user", ""), ("host", ""), ("port", "22")],
            "the first mention of a name sets its default"
        );
        assert!(placeholders("no {single} braces").is_empty());
        let values = [
            ("user".to_owned(), "me".to_owned()),
            ("host".to_owned(), "box".to_owned()),
        ];
        assert_eq!(
            fill(text, &values),
            "ssh me@box -p {{ port = 22 }} # me again, {{}} and {{open",
            "known names filled, the rest left as written"
        );
        assert_eq!(fill("plain", &values), "plain");
    }

    #[test]
    fn remove_tag_strips_it_everywhere_and_reports_who_had_it() {
        let mut v = Vault::default();
        v.commands.push(Command::new(draft("a", "ls")));
        v.commands.push(Command::new(draft("b", "ls -l")));
        v.commands[1].tags.push("shell".into());
        v.ensure_tags();
        let (tag, ids) = v.remove_tag("fs").expect("fs exists");
        assert_eq!(tag.name, "fs");
        assert_eq!(ids, [v.commands[0].id, v.commands[1].id]);
        assert!(v.commands[0].tags.is_empty());
        assert_eq!(v.commands[1].tags, ["shell"]);
        assert!(v.tag_colour("fs").is_none());
        assert!(v.remove_tag("fs").is_none(), "gone already");
    }

    #[test]
    fn rename_tag_refuses_a_comma() {
        let mut v = Vault::default();
        v.commands.push(Command::new(draft("a", "ls")));
        v.ensure_tags();
        assert!(!v.rename_tag("fs", "a,b"), "the form splits tags on commas");
        assert_eq!(v.commands[0].tags, ["fs"]);
    }

    #[test]
    fn tag_usage_counts_commands_and_skips_unused_tags() {
        let mut v = Vault::default();
        v.commands.push(Command::new(draft("a", "ls")));
        v.commands.push(Command::new(draft("b", "ls -l")));
        v.commands[1].tags.push("shell".into());
        v.ensure_tags();
        v.tags.push(Tag {
            name: "old".into(),
            colour: PALETTE[0].into(),
        });
        let usage: Vec<(&str, usize)> = v
            .tag_usage()
            .into_iter()
            .map(|(t, n)| (t.name.as_str(), n))
            .collect();
        assert_eq!(usage, [("fs", 2), ("shell", 1)]);
    }

    #[test]
    fn apply_replaces_fields_and_keeps_created_at() {
        let mut c = Command::new(draft("old", "ls"));
        let (id, created) = (c.id, c.created_at);
        c.apply(draft("new", "ls -la"));
        assert_eq!((c.id, c.created_at), (id, created));
        assert_eq!(
            (c.title.as_str(), c.command_text.as_str()),
            ("new", "ls -la")
        );
        assert!(c.updated_at >= created);
    }

    #[test]
    fn draft_requires_title_and_command() {
        assert!(draft("t", "c").is_valid());
        assert!(draft("", "c").is_valid(), "the title is optional");
        assert!(!draft("t", "").is_valid());
        assert_eq!(title_for("echo one\necho two"), "echo one");
        assert_eq!(title_for(&"é".repeat(70)).chars().count(), 60);
    }

    #[test]
    fn tree_is_depth_first_and_sorted() {
        let (v, _, _, ssh) = vault_with_tree();
        let names: Vec<(usize, &str)> = v
            .tree()
            .iter()
            .map(|(d, c)| (*d, c.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [(0, "linux"), (1, "network"), (2, "ssh"), (0, "windows")]
        );
        assert_eq!(v.path_of(Some(ssh)), "linux / network / ssh");
        assert_eq!(v.path_of(None), "");
        assert_eq!(v.path_of(Some(Uuid::new_v4())), "");
    }

    #[test]
    fn is_within_walks_ancestors() {
        let (v, linux, network, ssh) = vault_with_tree();
        assert!(v.is_within(Some(ssh), linux));
        assert!(v.is_within(Some(network), network));
        assert!(!v.is_within(Some(linux), ssh));
        assert!(!v.is_within(None, linux));
    }

    #[test]
    fn remove_category_reparents_children_and_commands() {
        let (mut v, linux, network, ssh) = vault_with_tree();
        let mut d = draft("t", "c");
        d.category_id = Some(network);
        v.commands.push(Command::new(d));

        v.remove_category(network);

        assert_eq!(v.parent_of(ssh), Some(linux));
        assert_eq!(v.commands[0].category_id, Some(linux));
        assert!(v.categories.iter().all(|c| c.id != network));
    }

    #[test]
    fn update_category_renames_moves_and_refuses_cycles() {
        let (mut v, linux, network, ssh) = vault_with_tree();
        assert!(v.update_category(ssh, "openssh".into(), Some(linux)));
        assert_eq!(v.parent_of(ssh), Some(linux));
        let renamed = v.categories.iter().find(|c| c.id == ssh).unwrap();
        assert_eq!(renamed.name, "openssh");

        assert!(!v.update_category(linux, "linux".into(), Some(network)));
        assert!(!v.update_category(linux, "linux".into(), Some(linux)));
        assert_eq!(v.parent_of(linux), None);
    }

    #[test]
    fn ensure_tags_registers_each_name_once_with_a_stable_colour() {
        let mut v = Vault::default();
        let mut a = draft("a", "c");
        a.tags = vec!["fs".into(), "net".into()];
        let mut b = draft("b", "c");
        b.tags = vec!["net".into()];
        v.commands.push(Command::new(a));
        v.commands.push(Command::new(b));

        v.ensure_tags();
        assert_eq!(v.tags.len(), 2);
        let net = v.tag_colour("net").unwrap().to_owned();
        assert!(PALETTE.contains(&net.as_str()));
        assert_eq!(net, palette_colour("net"));

        v.tags[0].colour = "#123456".into();
        v.ensure_tags();
        assert_eq!(v.tags.len(), 2, "idempotent");
        assert_eq!(v.tags[0].colour, "#123456", "existing colour kept");
        assert_eq!(v.tag_colour("missing"), None);

        assert!(!v.cycle_tag_colour("missing"));
        assert!(
            v.cycle_tag_colour("fs"),
            "custom colour goes to the first palette entry"
        );
        assert_eq!(v.tag_colour("fs"), Some(PALETTE[0]));
        assert!(v.cycle_tag_colour("fs"));
        assert_eq!(v.tag_colour("fs"), Some(PALETTE[1]));
    }

    #[test]
    fn rename_tag_updates_commands_and_merges_onto_existing() {
        let mut v = Vault::default();
        let mut a = draft("a", "c");
        a.tags = vec!["fs".into(), "net".into()];
        v.commands.push(Command::new(a));
        v.ensure_tags();
        let fs_colour = v.tag_colour("fs").unwrap().to_owned();

        assert!(!v.rename_tag("fs", " "));
        assert!(!v.rename_tag("fs", "fs"));
        assert!(!v.rename_tag("nope", "x"));

        assert!(v.rename_tag("fs", "files"));
        assert_eq!(v.commands[0].tags, ["net", "files"]);
        assert_eq!(v.tag_colour("files"), Some(fs_colour.as_str()));
        assert_eq!(v.tag_colour("fs"), None);

        assert!(v.rename_tag("files", "net"), "merge onto an existing tag");
        assert_eq!(v.commands[0].tags, ["net"]);
        assert_eq!(v.tags.len(), 1);
    }

    #[test]
    fn old_vault_without_categories_still_loads() {
        let json = r#"{"commands":[{"id":"11111111-1111-4111-8111-111111111111","title":"t","description":"","command_text":"c","tags":[],"created_at":"2026-09-10T17:00:00Z","updated_at":"2026-09-10T17:00:00Z"}]}"#;
        let v: Vault = serde_json::from_str(json).unwrap();
        assert_eq!(v.commands[0].category_id, None);
        assert_eq!(v.commands[0].copies, 0);
        assert!(v.categories.is_empty());
        assert!(v.tags.is_empty());
    }
}
