#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use image::{Rgb, RgbImage};
use mdlive::{Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui_image::picker::Picker;
use unicode_width::UnicodeWidthStr;

mod common;

use common::{HEADER_ROWS, below_header};

const RED: Rgb<u8> = Rgb([255, 0, 0]);
const BLUE: Rgb<u8> = Rgb([0, 0, 255]);

/// Writes a picture whose top `split` rows are `top` and the rest `bottom`.
fn picture(path: &Path, width: u32, height: u32, split: u32, top: Rgb<u8>, bottom: Rgb<u8>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    RgbImage::from_fn(width, height, |_, y| if y < split { top } else { bottom })
        .save(path)
        .unwrap();
}

/// A viewer for a page in `dir`, drawing pictures in half blocks on 10 by 20 pixel cells.
fn viewer(dir: &Path, markdown: &str) -> Viewer {
    Viewer::new("doc.md", markdown).with_images(dir, Picker::halfblocks())
}

fn draw(viewer: &mut Viewer, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height + HEADER_ROWS)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    below_header(terminal.backend().buffer())
}

fn row(buffer: &Buffer, y: u16) -> String {
    let mut text = String::new();
    let mut x = 0;
    while x < buffer.area.width {
        let symbol = buffer[(x, y)].symbol();
        text.push_str(symbol);
        x += u16::try_from(symbol.width().max(1)).unwrap();
    }
    text.trim_end().to_string()
}

/// The columns of row `y` whose lower half is painted `colour`.
fn painted(buffer: &Buffer, y: u16, colour: Rgb<u8>) -> Vec<u16> {
    let [r, g, b] = colour.0;
    (0..buffer.area.width)
        .filter(|&x| buffer[(x, y)].bg == Color::Rgb(r, g, b))
        .collect()
}

/// A picture beside the page is drawn at the size a docs site shows it, 8 picture pixels to a
/// column, in the rows it takes in the text.
///
/// Why it matters: the path is relative to the Markdown file, not to where the viewer started,
/// and a picture drawn over the text after it hides that text.
#[test]
fn draws_a_local_picture_at_docs_site_size_between_the_blocks() {
    // Given: an 80 by 40 pixel picture in a folder beside the page, on 10 by 20 pixel cells
    let dir = tempfile::tempdir().unwrap();
    picture(&dir.path().join("pics/red.png"), 80, 40, 40, RED, RED);
    let markdown = "before\n\n![red](pics/red.png)\n\nafter\n";

    // When: the page is drawn
    let buffer = draw(&mut viewer(dir.path(), markdown), 40, 10);

    // Then: the picture is 100 by 50 screen pixels, so it takes 10 columns and 3 rows, with the
    // text around it in place
    assert_eq!(row(&buffer, 0), " before");
    for y in 2..5 {
        assert_eq!(painted(&buffer, y, RED), (1..11).collect::<Vec<_>>());
    }
    assert_eq!(row(&buffer, 5), "");
    assert_eq!(row(&buffer, 6), " after");
}

/// A picture wider than the text shrinks to the text width and keeps its shape, and a picture in
/// a list sits after the bullet.
///
/// Why it matters: a picture wider than the screen would be cut off, and one drawn over the
/// bullet hides which item it belongs to.
#[test]
fn fits_a_wide_picture_to_the_text_and_a_list_picture_after_its_bullet() {
    // Given: a 2000 by 400 pixel picture, and a 16 by 16 one in a list item
    let dir = tempfile::tempdir().unwrap();
    picture(&dir.path().join("wide.png"), 2000, 400, 400, BLUE, BLUE);
    picture(&dir.path().join("dot.png"), 16, 16, 16, RED, RED);
    let markdown = "![wide](wide.png)\n\n- ![dot](dot.png)\n";

    // When: the page is drawn 40 columns wide, which leaves 38 for the text
    let buffer = draw(&mut viewer(dir.path(), markdown), 40, 8);

    // Then: the wide picture fills the 38 columns, and its height shrinks by the same scale
    for y in 0..4 {
        assert_eq!(painted(&buffer, y, BLUE), (1..39).collect::<Vec<_>>());
    }
    assert_eq!(painted(&buffer, 4, BLUE), Vec::<u16>::new());
    // And: the dot follows the bullet
    assert!(row(&buffer, 5).starts_with(" •"));
    assert_eq!(painted(&buffer, 5, RED), [3, 4]);
}

/// A picture scrolled partly past the top shows its lower part.
///
/// Why it matters: showing the top part instead would draw the wrong rows of the picture beside
/// the text, and the picture would seem to slide over the page as the reader scrolls.
#[test]
fn shows_the_lower_part_of_a_picture_scrolled_past_the_top() {
    // Given: a picture 4 rows tall, red in the top half and blue in the bottom half
    let dir = tempfile::tempdir().unwrap();
    picture(&dir.path().join("tall.png"), 8, 64, 32, RED, BLUE);
    let mut viewer = viewer(dir.path(), "![tall](tall.png)\n\nafter\n\nmore\n\nend\n");
    draw(&mut viewer, 40, 6);

    // When: the reader scrolls three lines down
    for _ in 0..3 {
        viewer.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    }
    let buffer = draw(&mut viewer, 40, 6);

    // Then: the first row shows the last row of the picture
    assert_eq!(painted(&buffer, 0, BLUE), [1]);
    assert_eq!(row(&buffer, 2), " after");
}

/// A picture that cannot be shown leaves its alt text and the reason, and a picture inside a line
/// of text reads as its alt text.
///
/// Why it matters: a page must never make a network call when it opens, and a reader who sees
/// nothing where a picture should be cannot tell a typo in the path from a broken file.
#[test]
fn shows_the_alt_text_and_the_reason_for_a_picture_it_cannot_draw() {
    // Given: a remote picture, a missing one, a file that is not a picture, and one in a sentence
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("bad.png"), "not a picture").unwrap();
    picture(&dir.path().join("red.png"), 10, 20, 20, RED, RED);
    let markdown = "![Remote](https://example.com/a.png)\n\n![Missing](nope.png)\n\n\
                    ![Broken](bad.png)\n\nsee ![icon](red.png) here\n\n![icon](red.png) first\n\nlast ![icon](red.png)\n";

    // When: the page is drawn
    let buffer = draw(&mut viewer(dir.path(), markdown), 60, 16);

    // Then: each picture that cannot be drawn shows its alt text and the reason in the muted colour
    assert_eq!(row(&buffer, 0), " ▣ Remote (remote image, not fetched)");
    assert_eq!(row(&buffer, 2), " ▣ Missing (not found: nope.png)");
    assert!(
        row(&buffer, 4).starts_with(" ▣ Broken (cannot read: "),
        "{}",
        row(&buffer, 4)
    );
    assert_eq!(buffer[(10, 0)].fg, theme::MUTED);
    // And: a picture in a sentence reads as its alt text, at either end of the sentence too
    for sentence in [" see icon here", " icon first", " last icon"] {
        let y = (0..buffer.area.height)
            .find(|&y| row(&buffer, y) == sentence)
            .unwrap_or_else(|| panic!("{sentence:?} is not on screen"));
        assert_eq!(painted(&buffer, y, RED), Vec::<u16>::new());
    }
}

/// A picture under the contents panel is not drawn.
///
/// Why it matters: a graphics protocol draws the picture over the text cells, so on a narrow
/// screen it would cover the contents panel the reader just opened.
#[test]
fn leaves_out_a_picture_under_the_contents_panel() {
    // Given: a picture on a narrow screen, and the contents open over the page
    let dir = tempfile::tempdir().unwrap();
    picture(&dir.path().join("wide.png"), 500, 40, 40, RED, RED);
    let mut viewer = viewer(dir.path(), "# Title\n\n![wide](wide.png)\n");
    viewer.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // When: the page is drawn 60 columns wide
    let buffer = draw(&mut viewer, 60, 8);

    // Then: no part of the picture shows beside the panel
    for y in 0..7 {
        assert_eq!(painted(&buffer, y, RED), Vec::<u16>::new());
    }
}

/// A picture under the key popup is not drawn while the popup is open.
///
/// Why it matters: a graphics protocol draws the picture over the text cells, so it would cover the
/// keys the popup lists.
#[test]
fn leaves_out_a_picture_under_the_key_popup() {
    // Given: a wide picture on the page
    let dir = tempfile::tempdir().unwrap();
    picture(&dir.path().join("wide.png"), 500, 40, 40, RED, RED);
    let mut viewer = viewer(dir.path(), "# Title\n\n![wide](wide.png)\n");
    let red_rows = |viewer: &mut Viewer| {
        let buffer = draw(viewer, 60, 9);
        (0..9)
            .filter(|&y| !painted(&buffer, y, RED).is_empty())
            .count()
    };
    // Then: the picture shows
    assert!(red_rows(&mut viewer) > 0);

    // When: the reader presses z, and the popup opens over the picture
    viewer.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    // Then: no part of the picture shows
    assert_eq!(red_rows(&mut viewer), 0);

    // When: the reader presses Escape
    viewer.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    // Then: the picture shows again
    assert!(red_rows(&mut viewer) > 0);
}

/// After the reader opens a page in another folder, its pictures resolve from that folder.
///
/// Why it matters: a page names its pictures relative to itself, so a viewer that keeps the folder
/// of the first page shows every picture of the next page as not found.
#[test]
fn reads_the_pictures_of_an_opened_page_from_its_folder() {
    // Given: a first page, and a second page in a subfolder with a picture beside it
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("first.md"), "first\n").unwrap();
    picture(&dir.path().join("sub/red.png"), 80, 32, 32, RED, RED);
    let second = dir.path().join("sub/second.md");
    std::fs::write(&second, "![red](red.png)\n").unwrap();
    let mut viewer = viewer(dir.path(), "first\n").with_path(&dir.path().join("first.md"));
    draw(&mut viewer, 40, 8);

    // When: the reader opens the second page
    viewer.open(&second);
    let buffer = draw(&mut viewer, 40, 8);

    // Then: the picture beside it is drawn
    assert_eq!(painted(&buffer, 0, RED), (1..11).collect::<Vec<_>>());
}

/// A site-root path finds the picture in the site's public folder or at the repository root, and
/// an SVG is drawn at its size.
///
/// Why it matters: docs sites serve `/diagrams/a.svg` from `public/`, so a page in a nested docs
/// folder names its pictures that way, and most of those pictures are SVG diagrams.
#[test]
fn draws_site_root_and_svg_pictures_of_a_docs_page() {
    // Given: a repository with a picture in public/, one at its root, and an SVG, and a page two
    // folders down that names each from the site root
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    picture(
        &repo.path().join("public/diagrams/red.png"),
        80,
        16,
        16,
        RED,
        RED,
    );
    picture(&repo.path().join("img/blue.png"), 80, 16, 16, BLUE, BLUE);
    std::fs::write(
        repo.path().join("public/diagrams/red.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="32"><rect width="80" height="32" fill="#ff0000"/></svg>"##,
    )
    .unwrap();
    let page = repo.path().join("docs/guide");
    std::fs::create_dir_all(&page).unwrap();
    let markdown = "![a](/diagrams/red.png)\n\n![b](/img/blue.png)\n\n![c](/diagrams/red.svg)\n";

    // When: the page is drawn
    let buffer = draw(&mut viewer(&page, markdown), 40, 10);

    // Then: each picture is drawn at docs site size
    let columns: Vec<u16> = (1..11).collect();
    assert_eq!(painted(&buffer, 0, RED), columns);
    assert_eq!(painted(&buffer, 2, BLUE), columns);
    assert_eq!(painted(&buffer, 4, RED), columns);
    assert_eq!(painted(&buffer, 5, RED), columns);
}

/// A site-root path does not look above the repository.
///
/// Why it matters: a page outside the site would otherwise show a picture from an unrelated
/// project higher up the disk.
#[test]
fn stops_looking_for_a_site_root_picture_at_the_repository() {
    // Given: a picture above a repository, and a page in the repository that names it from the root
    let outer = tempfile::tempdir().unwrap();
    picture(&outer.path().join("public/red.png"), 100, 20, 20, RED, RED);
    let repo = outer.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    // When: the page is drawn
    let buffer = draw(&mut viewer(&repo, "![a](/red.png)\n"), 40, 4);

    // Then: the picture is not found
    assert_eq!(row(&buffer, 0), " ▣ a (not found: /red.png)");
}
