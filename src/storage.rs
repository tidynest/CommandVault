use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// `$XDG_CONFIG_HOME/commandvault/vault.json`, falling back to `~/.config`.
/// A relative `XDG_CONFIG_HOME` is ignored, as the spec requires.
pub fn vault_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::home_dir().map(|h| h.join(".config")))?;
    Some(base.join("commandvault").join("vault.json"))
}

/// The path for a status line, with the home directory shortened to `~`.
pub fn tidy(path: &Path) -> String {
    let shown = path.display().to_string();
    env::home_dir()
        .and_then(|home| {
            shown
                .strip_prefix(&home.display().to_string())
                .map(|rest| format!("~{rest}"))
        })
        .unwrap_or(shown)
}

/// A missing file is the default value. A corrupt file is an error, never silently replaced.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e),
    }
}

/// Writes a sibling temp file, syncs it, keeps the previous vault as `vault.json.bak`,
/// then renames the temp file over the target. A crash at any point leaves either the
/// old vault or the new one in place, never a partial file. The vault can hold secrets
/// typed into commands, so the file and its directory are private to the user.
pub fn save<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        create_private_dir(dir)?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    let mut file = private_file_options().open(&tmp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    if path.exists() {
        fs::copy(path, path.with_extension("json.bak"))?;
    }
    fs::rename(tmp, path)
}

/// When the file was last written, `None` when it does not exist. Compared before a
/// save to notice another program writing the file in between.
pub fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Dated copies kept beside the vault. The oldest go when a new one is made.
pub const KEPT_COPIES: usize = 10;

/// Copies the file beside itself as `<name>.<epoch seconds>.bak`, for a version another
/// program wrote that is about to be overwritten, then prunes those copies to
/// `KEPT_COPIES`. Returns the copy's path. A failed prune is logged, the copy stands.
pub fn keep_copy(path: &Path) -> io::Result<PathBuf> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let kept = path.with_extension(format!("json.{secs}.bak"));
    fs::copy(path, &kept)?;
    if let Err(e) = prune_copies(path) {
        log::warn!("could not prune old copies of {}: {e}", path.display());
    }
    Ok(kept)
}

/// Removes every `<name>.<epoch>.bak` beside `path` but the newest `KEPT_COPIES`.
/// `<name>.bak` and anything without a number in that slot is left alone.
fn prune_copies(path: &Path) -> io::Result<()> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return Ok(());
    };
    let mut dated: Vec<(u64, PathBuf)> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name();
            let secs = file
                .to_str()?
                .strip_prefix(name)?
                .strip_prefix('.')?
                .strip_suffix(".bak")?
                .parse()
                .ok()?;
            Some((secs, entry.path()))
        })
        .collect();
    dated.sort_unstable_by_key(|(secs, _)| std::cmp::Reverse(*secs));
    dated
        .iter()
        .skip(KEPT_COPIES)
        .try_for_each(|(_, old)| fs::remove_file(old))
}

/// Plain text with the same private mode as the vault, for exports.
pub fn write_private(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        create_private_dir(dir)?;
    }
    private_file_options()
        .open(path)?
        .write_all(text.as_bytes())
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Command, Draft, Vault};

    #[test]
    fn tidy_shortens_the_home_directory() {
        let home = env::home_dir().expect("a home directory in tests");
        assert_eq!(tidy(&home.join(".config/x.json")), "~/.config/x.json");
        assert_eq!(tidy(Path::new("/tmp/x.json")), "/tmp/x.json");
    }

    fn vault_with(command_text: &str) -> Vault {
        Vault {
            commands: vec![Command::new(Draft {
                title: "t".into(),
                command_text: command_text.into(),
                ..Draft::default()
            })],
            categories: Vec::new(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn missing_round_trip_backup_and_corrupt() {
        let dir = env::temp_dir().join(format!("commandvault-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("vault.json");

        assert!(load::<Vault>(&path).unwrap().commands.is_empty());

        save(&path, &vault_with("echo one")).unwrap();
        assert_eq!(
            load::<Vault>(&path).unwrap().commands[0].command_text,
            "echo one"
        );
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file is renamed away"
        );
        assert!(
            !path.with_extension("json.bak").exists(),
            "no backup before a second save"
        );

        save(&path, &vault_with("echo two")).unwrap();
        assert_eq!(
            load::<Vault>(&path).unwrap().commands[0].command_text,
            "echo two"
        );
        let bak: Vault = load(&path.with_extension("json.bak")).unwrap();
        assert_eq!(bak.commands[0].command_text, "echo one");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(&dir), 0o700);
        }

        fs::write(&path, b"not json").unwrap();
        assert!(load::<Vault>(&path).is_err());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keep_copy_prunes_dated_copies_past_the_cap() {
        let dir = env::temp_dir().join(format!("commandvault-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("vault.json");
        save(&path, &vault_with("echo one")).unwrap();
        save(&path, &vault_with("echo two")).unwrap();
        let dated = |n: usize| path.with_extension(format!("json.{n}.bak"));
        // Two past the cap, and the new copy makes it three over.
        for n in 1..=KEPT_COPIES + 2 {
            fs::write(dated(n), b"{}").unwrap();
        }
        let notes = path.with_extension("json.notes.bak");
        fs::write(&notes, b"mine").unwrap();

        let kept = keep_copy(&path).unwrap();
        assert!(kept.exists(), "the new copy is the newest");
        for n in 1..=3 {
            assert!(!dated(n).exists(), "copy {n} is past the cap");
        }
        for n in 4..=KEPT_COPIES + 2 {
            assert!(dated(n).exists(), "copy {n} is within the cap");
        }
        assert!(notes.exists(), "a copy without an epoch is not ours");
        assert!(
            path.with_extension("json.bak").exists(),
            "the plain backup stays"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
