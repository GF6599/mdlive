use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use mermaid_rs_renderer::{RenderOptions, Theme};

/// Mermaid blocks drawn as SVG, kept by their source because one takes about 20 ms to draw and
/// a page is laid out again on every save, resize and fold.
#[derive(Default)]
pub struct Diagrams {
    drawn: RefCell<HashMap<String, Result<Arc<str>, String>>>,
    used: RefCell<HashSet<String>>,
}

impl fmt::Debug for Diagrams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Diagrams")
            .field("drawn", &self.drawn.borrow().len())
            .finish()
    }
}

impl Diagrams {
    pub fn svg(&self, source: &str) -> Result<Arc<str>, String> {
        self.used.borrow_mut().insert(source.to_string());
        self.drawn
            .borrow_mut()
            .entry(source.to_string())
            .or_insert_with(|| draw_svg(source))
            .clone()
    }

    /// Forgets the diagrams the last layout did not use, so edits to a block do not pile up.
    pub fn forget_unused(&self) {
        let used = std::mem::take(&mut *self.used.borrow_mut());
        self.drawn
            .borrow_mut()
            .retain(|source, _| used.contains(source));
    }
}

fn draw_svg(source: &str) -> Result<Arc<str>, String> {
    let options = RenderOptions {
        theme: Theme {
            background: String::from("transparent"),
            ..Theme::dark()
        },
        ..RenderOptions::default()
    };
    mermaid_rs_renderer::render_with_options(source, options)
        .map(Arc::from)
        .map_err(|error| error.to_string())
}

/// The diagram as lines of box drawing, fitted to `width` columns where the layout allows.
pub fn text(source: &str, width: usize) -> Result<Vec<String>, String> {
    mermaid_text::render_with_width(source, Some(width))
        .map(|text| {
            text.lines()
                .map(|line| line.trim_end().to_string())
                .collect()
        })
        .map_err(|error| error.to_string())
}
