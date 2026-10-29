#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use mdlive::config::Config;
use mdlive::theme;

/// mdlive started on a page in `dir`, with `env` as its only config and home folders.
fn mdlive(dir: &Path, env: &[(&str, PathBuf)]) -> Output {
    std::fs::write(dir.join("page.md"), "# Page\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_mdlive"));
    command
        .arg(dir.join("page.md"))
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn config_file(folder: &Path, text: &str) -> PathBuf {
    let path = folder.join("mdlive/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

/// A config file that names an unknown theme, holds an unknown key or does not parse stops mdlive
/// at startup, with the path of the file, the problem and the theme names.
///
/// Why it matters: a typo in the file would otherwise start mdlive in a theme the reader did not
/// choose, with nothing to say why.
#[test]
fn stops_at_startup_on_a_config_file_it_cannot_use() {
    let dir = tempfile::tempdir().unwrap();
    let xdg = dir.path().join("xdg");
    for (text, problem) in [
        ("theme = \"sepia\"\n", "unknown theme \"sepia\""),
        ("colour = \"rose-pine\"\n", "unknown field `colour`"),
        ("theme = rose-pine\n", "line 1"),
    ] {
        // Given: the config file in the folder that XDG_CONFIG_HOME names
        let path = config_file(&xdg, text);

        // When: mdlive starts
        let output = mdlive(dir.path(), &[("XDG_CONFIG_HOME", xdg.clone())]);

        // Then: it stops, and says which file, what is wrong and which themes there are
        let message = String::from_utf8(output.stderr).unwrap();
        assert!(!output.status.success(), "{text} started");
        assert!(message.contains(&path.display().to_string()), "{message}");
        assert!(message.contains(problem), "{message}");
        assert!(message.contains("The themes are: rose-pine"), "{message}");
    }

    // Given: no XDG_CONFIG_HOME, and a config file in .config under the home folder
    let home = dir.path().join("home");
    let path = config_file(&home.join(".config"), "theme = \"sepia\"\n");
    // When: mdlive starts
    let output = mdlive(dir.path(), &[("HOME", home)]);
    // Then: that is the file it reads
    let message = String::from_utf8(output.stderr).unwrap();
    assert!(message.contains(&path.display().to_string()), "{message}");
}

/// With no config file, or with one that names it, the theme is Rose Pine.
///
/// Why it matters: a reader who never makes the file must still get the default theme, and not
/// an error.
#[test]
fn takes_rose_pine_with_no_config_file_or_one_that_names_it() {
    let dir = tempfile::tempdir().unwrap();

    // When: the config is read from a folder with no file
    let config = Config::read(&dir.path().join("mdlive/config.toml")).unwrap();
    // Then: the theme is Rose Pine
    assert_eq!(config.palette.name, theme::ROSE_PINE.name);

    // When: the file names Rose Pine
    let path = config_file(dir.path(), "theme = \"rose-pine\"\n");
    // Then: the theme is Rose Pine
    assert_eq!(
        Config::read(&path).unwrap().palette.name,
        theme::ROSE_PINE.name
    );
}
