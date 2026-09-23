use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage;

/// Order of the command list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    #[default]
    Title,
    /// Most recently updated first.
    Recent,
    /// Most copied first, then by title.
    Popular,
    /// Last copied first, never copied last by title.
    Copied,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Title, Sort::Recent, Sort::Popular, Sort::Copied];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "title" => Some(Sort::Title),
            "recent" => Some(Sort::Recent),
            "popular" => Some(Sort::Popular),
            "copied" => Some(Sort::Copied),
            _ => None,
        }
    }
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Sort::Title => "By title",
            Sort::Recent => "Recently updated",
            Sort::Popular => "Most copied",
            Sort::Copied => "Recently copied",
        })
    }
}

/// Which colours: `System` asks iced for the desktop's preference, `Theme` is one of
/// iced's built-in themes by name. Stored as that name in lowercase, an unknown name
/// loads as `System`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Scheme {
    #[default]
    System,
    Theme(iced::Theme),
}

impl Scheme {
    /// `System` first, then every built-in theme in iced's order.
    pub fn all() -> Vec<Scheme> {
        std::iter::once(Scheme::System)
            .chain(iced::Theme::ALL.iter().cloned().map(Scheme::Theme))
            .collect()
    }

    /// `None` lets iced follow the system, which is what the `.theme` callback expects.
    pub fn theme(&self) -> Option<iced::Theme> {
        match self {
            Scheme::System => None,
            Scheme::Theme(t) => Some(t.clone()),
        }
    }
}

impl From<String> for Scheme {
    fn from(name: String) -> Self {
        iced::Theme::ALL
            .iter()
            .find(|t| t.to_string().eq_ignore_ascii_case(&name))
            .cloned()
            .map_or(Scheme::System, Scheme::Theme)
    }
}

impl From<Scheme> for String {
    fn from(scheme: Scheme) -> Self {
        match scheme {
            Scheme::System => "system".into(),
            Scheme::Theme(t) => t.to_string().to_lowercase(),
        }
    }
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scheme::System => f.write_str("System"),
            Scheme::Theme(t) => t.fmt(f),
        }
    }
}

/// Logical window size, written when the window closes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowSize {
    pub width: f32,
    pub height: f32,
}

/// Everything in `config.json`. Every field has a default so an older file still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub sort: Sort,
    pub scheme: Scheme,
    /// `None` until the app has been closed once, then the size at that close.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowSize>,
    /// Scale factor for the whole UI, 1.0 is the desktop's own. Ctrl+Up and Down step it.
    pub zoom: f32,
    /// The sidebar selection, restored at startup while the category still exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Uuid>,
    /// Whether the category and tag column is shown. Ctrl+B toggles it.
    pub sidebar: bool,
    /// Seconds after a copy until the clipboard is wiped, if it still holds the copy.
    /// Commands carry tokens. 0 never wipes.
    pub clear_after: u64,
    /// Rows without description, path and chips, so more of them fit. Ctrl+Shift+B.
    pub compact: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sort: Sort::default(),
            scheme: Scheme::default(),
            window: None,
            zoom: 1.0,
            category: None,
            sidebar: true,
            clear_after: 30,
            compact: false,
        }
    }
}

/// `config.json` in the same directory as the vault.
pub fn config_path(vault_path: &Path) -> PathBuf {
    vault_path.with_file_name("config.json")
}

/// The file, then `COMMANDVAULT_SORT` on top as the startup value. A bad or unreadable
/// file falls back to defaults, settings are not precious the way the vault is.
pub fn load(path: &Path) -> Settings {
    let mut settings = storage::load::<Settings>(path).unwrap_or_else(|e| {
        log::warn!("could not read {}: {e}, using defaults", path.display());
        Settings::default()
    });
    if let Some(value) = env::var_os("COMMANDVAULT_SORT") {
        let value = value.to_string_lossy();
        match Sort::parse(&value) {
            Some(sort) => settings.sort = sort,
            None => log::warn!(
                "ignoring COMMANDVAULT_SORT={value:?}, expected title, recent, popular or copied"
            ),
        }
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_lenient_about_case_and_space() {
        assert_eq!(Sort::parse(" Recent "), Some(Sort::Recent));
        assert_eq!(Sort::parse("TITLE"), Some(Sort::Title));
        assert_eq!(Sort::parse("popular"), Some(Sort::Popular));
        assert_eq!(Sort::parse("copied"), Some(Sort::Copied));
        assert_eq!(Sort::parse("newest"), None);
    }

    #[test]
    fn settings_round_trip_and_unknown_fields_are_ignored() {
        let json = serde_json::to_string(&Settings {
            sort: Sort::Recent,
            scheme: Scheme::Theme(iced::Theme::Dark),
            window: None,
            zoom: 1.0,
            category: None,
            sidebar: false,
            clear_after: 0,
            compact: true,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"sort":"recent","scheme":"dark","zoom":1.0,"sidebar":false,"clear_after":0,"compact":true}"#,
            "no window key until a close"
        );
        let sized = Settings {
            window: Some(WindowSize {
                width: 1280.0,
                height: 800.0,
            }),
            ..Settings::default()
        };
        let json = serde_json::to_string(&sized).unwrap();
        assert_eq!(
            json,
            r#"{"sort":"title","scheme":"system","window":{"width":1280.0,"height":800.0},"zoom":1.0,"sidebar":true,"clear_after":30,"compact":false}"#
        );
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), sized);
        let back: Settings = serde_json::from_str(r#"{"sort":"recent","future":1}"#).unwrap();
        assert_eq!(back.sort, Sort::Recent);
        assert_eq!(
            back.scheme,
            Scheme::System,
            "a file without scheme keeps following the system"
        );
        let empty: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Settings::default());
        assert_eq!(
            empty.zoom, 1.0,
            "a file without zoom is not zoomed to nothing"
        );
        assert!(empty.sidebar, "a file without sidebar shows it");
        assert_eq!(
            empty.clear_after, 30,
            "a file without clear_after wipes after 30 s"
        );
        assert!(!empty.compact, "full rows unless asked");
    }

    #[test]
    fn scheme_names_any_built_in_theme_and_falls_back_to_system() {
        let storm = Scheme::Theme(iced::Theme::TokyoNightStorm);
        let json = serde_json::to_string(&storm).unwrap();
        assert_eq!(json, r#""tokyo night storm""#);
        assert_eq!(serde_json::from_str::<Scheme>(&json).unwrap(), storm);
        assert_eq!(
            serde_json::from_str::<Scheme>(r#""Tokyo Night Storm""#).unwrap(),
            storm,
            "case does not matter"
        );
        assert_eq!(
            serde_json::from_str::<Scheme>(r#""system""#).unwrap(),
            Scheme::System
        );
        assert_eq!(
            serde_json::from_str::<Scheme>(r#""neon""#).unwrap(),
            Scheme::System,
            "an unknown name follows the desktop"
        );
        assert_eq!(Scheme::all().len(), iced::Theme::ALL.len() + 1);
        assert_eq!(Scheme::all()[0], Scheme::System);
        assert_eq!(storm.to_string(), "Tokyo Night Storm");
        assert_eq!(Scheme::System.theme(), None);
        assert_eq!(storm.theme(), Some(iced::Theme::TokyoNightStorm));
    }
}
