//! What an app's tables hold on the first morning.
//!
//! An accounting app whose first screen is "you have no chart of accounts,
//! create one" has shipped a blank page and called it flexibility. Almost
//! nobody wants to design a chart of accounts, and the people who do want to
//! are the ones who will change it anyway.
//!
//! So an app ships the rows a workspace starts with in
//! `config/defaults/<app_id>.toml`, reviewed in a pull request, the same
//! discipline as `config/numbering/`. They are inserted when the app is first
//! enabled, with `ON CONFLICT DO NOTHING`, and after that **the workspace owns
//! them**: a redeploy never puts back a row somebody deleted and never
//! overwrites one they edited.
//!
//! # Why this is generic and `numbering` is not
//!
//! Every app's number series have the same shape, so [`crate::numbering`] can
//! name it. Defaults do not — a chart of accounts and a set of stock adjustment
//! types have nothing in common but the file they live in. This module
//! therefore knows only how to find and parse the file; the *shape* is a type
//! the calling app declares, and so are the rules about what makes one valid.
//!
//! That is what keeps `phonix-config` free of any app dependency, which is the
//! whole reason it can sit beneath all of them.
//!
//! ```ignore
//! let chart: DefaultChart = phonix_config::defaults::load_for("books")?;
//! ```
//!
//! # Missing is not an error
//!
//! Most apps have nothing to seed. A file that is not there deserialises as
//! `T::default()`, so installing an app is never conditional on one existing.

use std::path::{Path, PathBuf};

use config::{Config, File, FileFormat};
use serde::de::DeserializeOwned;

/// The directory under `config/` these files live in.
pub const DIRECTORY: &str = "defaults";

/// Read one app's defaults from the workspace's own `config/defaults`.
pub fn load_for<T>(app_id: &str) -> Result<T, DefaultsError>
where
    T: DeserializeOwned + Default,
{
    load_from(
        crate::workspace_root().join("config").join(DIRECTORY),
        app_id,
    )
}

/// Read one app's defaults from an explicit directory.
///
/// Separated from [`load_for`] so tests can point at a fixture directory, the
/// same way [`crate::numbering::series_from`] is.
pub fn load_from<T>(dir: impl AsRef<Path>, app_id: &str) -> Result<T, DefaultsError>
where
    T: DeserializeOwned + Default,
{
    let path = dir.as_ref().join(format!("{app_id}.toml"));
    if !path.is_file() {
        // Most apps have nothing to seed.
        return Ok(T::default());
    }

    Config::builder()
        .add_source(File::from(path.clone()).format(FileFormat::Toml))
        .build()
        .map_err(|source| DefaultsError::Read {
            path: path.clone(),
            source: Box::new(source),
        })?
        .try_deserialize()
        .map_err(|source| DefaultsError::Read {
            path,
            source: Box::new(source),
        })
}

/// Why a defaults file was refused.
///
/// Stops the process at startup, like a bad numbering file: a chart installed
/// from a broken definition is one somebody has to unpick by hand afterwards,
/// and they will be unpicking it in a live workspace.
#[derive(Debug, thiserror::Error)]
pub enum DefaultsError {
    /// Unreadable, unparseable, or not the shape the app asked for.
    ///
    /// `source` is boxed for the reason [`crate::numbering::SeriesError`] boxes
    /// its own: `config::ConfigError` is large enough to set the size of every
    /// `Result` in the module.
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: Box<config::ConfigError>,
    },
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Default, Deserialize, PartialEq, Eq)]
    struct Thing {
        #[serde(default)]
        item: Vec<Item>,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Item {
        name: String,
    }

    /// Write one file into a fresh directory and read it back. Same shape as
    /// `numbering`'s fixture helper.
    fn load(name: &str, body: &str) -> Result<Thing, DefaultsError> {
        let dir = std::env::temp_dir().join(format!(
            "phonix-defaults-{}-{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&dir).expect("scratch directory");
        fs::write(dir.join("demo.toml"), body).expect("write fixture");
        let result = load_from(&dir, "demo");
        let _ = fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn an_app_with_no_file_starts_with_nothing_rather_than_failing() {
        let loaded: Thing = load_from(
            std::env::temp_dir().join("phonix-defaults-absent"),
            "nobody",
        )
        .expect("a missing file is not an error");

        assert_eq!(loaded, Thing::default());
    }

    #[test]
    fn a_file_is_read_into_the_shape_the_app_asked_for() {
        let loaded = load(
            "shape",
            "[[item]]\nname = \"one\"\n\n[[item]]\nname = \"two\"\n",
        )
        .expect("valid");

        assert_eq!(loaded.item.len(), 2);
        assert_eq!(loaded.item[0].name, "one");
    }

    #[test]
    fn a_file_of_the_wrong_shape_is_refused_at_load() {
        // Rather than at first use, which is in a live workspace.
        let loaded = load("typo", "[[item]]\nnmae = \"typo\"\n");

        assert!(matches!(loaded, Err(DefaultsError::Read { .. })));
    }
}
