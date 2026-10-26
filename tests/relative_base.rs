#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;

use image::{Rgb, RgbImage};
use mdlive::Viewer;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui_image::picker::Picker;

/// A page opened from its own folder still finds a site-root picture above that folder.
///
/// Why it matters: `mdl page.md` run in the page's folder names that folder as `.`, which has no
/// parent to walk up to, so the site's public folder would never be found.
#[test]
fn finds_a_site_root_picture_from_a_page_opened_in_its_own_folder() {
    // Given: a repository with a picture in public/, and the reader in a docs folder of it
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    let picture = repo.path().join("public/red.png");
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    RgbImage::from_pixel(100, 20, Rgb([255, 0, 0]))
        .save(&picture)
        .unwrap();
    let docs = repo.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    std::env::set_current_dir(&docs).unwrap();

    // When: the page is drawn with its folder named as `.`
    let mut viewer = Viewer::new("page.md", "![a](/red.png)\n")
        .with_images(Path::new("."), Picker::halfblocks());
    let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();

    // Then: the picture is drawn
    assert_eq!(
        terminal.backend().buffer()[(1, 0)].bg,
        Color::Rgb(255, 0, 0)
    );
}
