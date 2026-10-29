//! The reader's settings, from `mdlive/config.toml` in the config folder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::theme::{self, Palette};

#[derive(Debug)]
pub struct Config {
    pub palette: &'static Palette,
}

/// An unknown key is an error, because a misspelt key would otherwise be left out in silence.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    theme: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            palette: &theme::ROSE_PINE,
        }
    }
}

impl Config {
    /// An error names the file and lists the themes, so the reader can fix it from the message
    /// alone.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(problem(path, &error.to_string())),
        };
        let file: File =
            toml::from_str(&text).map_err(|error| problem(path, &error.to_string()))?;
        let Some(name) = file.theme else {
            return Ok(Self::default());
        };
        let palette = theme::named(&name)
            .ok_or_else(|| problem(path, &format!("unknown theme \"{name}\"")))?;
        Ok(Self { palette })
    }
}

/// macOS takes `~/.config` too, because terminal tools there keep their settings in it. The XDG
/// rules ignore a relative `XDG_CONFIG_HOME`.
pub fn path(config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let folder = config_home
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .or_else(|| home.map(|home| PathBuf::from(home).join(".config")))?;
    Some(folder.join("mdlive").join("config.toml"))
}

fn problem(path: &Path, problem: &str) -> String {
    let names: Vec<&str> = theme::THEMES.iter().map(|palette| palette.name).collect();
    format!(
        "{}: {}\nThe themes are: {}",
        path.display(),
        problem.trim_end(),
        names.join(", ")
    )
}
