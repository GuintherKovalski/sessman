//! Find in the main area (Ctrl+F): matches over the conversation's messages or an open file,
//! which one is current, and highlighting them in laid-out text.
use eframe::egui::text::{ByteIndex, CCursor, LayoutJob};
use eframe::egui::{self, Color32, Rect, Shape, layers::ShapeIdx};
use eframe::epaint::TextShape;
use std::ops::Range;

/// More than this many matches are not collected.
const MAX_MATCHES: usize = 10_000;

/// One match: which text it is in (a message, or 0 for a file) and its byte range there.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub item: usize,
    pub range: Range<usize>,
}

/// The find bar's state. Matches are cached for one (query, target, texts) combination.
#[derive(Default)]
pub struct Find {
    pub open: bool,
    pub query: String,
    /// Index into `matches` of the current one.
    pub current: usize,
    pub matches: Vec<Match>,
    /// Set when the current match changed, so the view scrolls to it once.
    pub scroll: bool,
    /// Set to focus (and select) the query field on the next frame.
    pub focus: bool,
    key: Option<(String, String, usize, usize)>,
}

impl Find {
    /// Ctrl+F: open the bar, or focus it again when it is already open.
    pub fn show(&mut self) {
        self.open = true;
        self.focus = true;
        self.scroll = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.matches.clear();
        self.key = None;
    }

    /// Matches for `texts` (what `target` shows), searched again only when something changed.
    /// A new target or query starts from the first match.
    pub fn update(&mut self, target: &str, texts: &[&str]) {
        let key = (self.query.clone(), target.to_owned(), texts.len(), texts.iter().map(|t| t.len()).sum());
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let fresh = self.key.as_ref().is_none_or(|k| k.0 != key.0 || k.1 != key.1);
        self.matches = find(texts, &self.query);
        self.key = Some(key);
        if fresh {
            self.current = 0;
            self.scroll = true;
        }
        self.current = self.current.min(self.matches.len().saturating_sub(1));
    }

    /// Next (or previous) match, wrapping around.
    pub fn step(&mut self, forward: bool) {
        let n = self.matches.len();
        if n == 0 {
            return;
        }
        self.current = if forward { (self.current + 1) % n } else { (self.current + n - 1) % n };
        self.scroll = true;
    }

    pub fn current_match(&self) -> Option<&Match> {
        self.matches.get(self.current)
    }

    /// "3 of 17", or "No results" for a query that matches nothing.
    pub fn status(&self) -> String {
        match self.matches.len() {
            _ if self.query.is_empty() => String::new(),
            0 => "No results".to_owned(),
            n if n >= MAX_MATCHES => format!("{} of {MAX_MATCHES}+", self.current + 1),
            n => format!("{} of {n}", self.current + 1),
        }
    }

    /// Ranges of the matches in one item, and which of them (if any) is the current one.
    pub fn in_item(&self, item: usize) -> (Vec<Range<usize>>, Option<usize>) {
        let mut current = None;
        let ranges = self
            .matches
            .iter()
            .enumerate()
            .filter(|(_, m)| m.item == item)
            .enumerate()
            .map(|(k, (i, m))| {
                if i == self.current {
                    current = Some(k);
                }
                m.range.clone()
            })
            .collect();
        (ranges, current)
    }
}

/// Every place `query` occurs in `texts`, ignoring ASCII case, without overlaps.
pub fn find(texts: &[&str], query: &str) -> Vec<Match> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    // ASCII lowercasing keeps byte offsets, so ranges index the original text.
    let q = query.to_ascii_lowercase();
    for (item, text) in texts.iter().enumerate() {
        let lower = text.to_ascii_lowercase();
        let mut from = 0;
        while let Some(at) = lower[from..].find(&q) {
            let start = from + at;
            out.push(Match { item, range: start..start + q.len() });
            if out.len() >= MAX_MATCHES {
                return out;
            }
            from = start + q.len();
        }
    }
    out
}

/// How matches look: a background (the current match's own), and the text over it.
#[derive(Clone, Copy)]
pub struct Colors {
    pub bg: Color32,
    pub current_bg: Color32,
    pub text: Color32,
}

/// Paints a background behind `ranges` of a laid-out job (the current one in its own colour),
/// splitting its sections at the match edges.
pub fn mark(job: &mut LayoutJob, ranges: &[Range<usize>], current: Option<usize>, colors: Colors) {
    if ranges.is_empty() {
        return;
    }
    let mut out = Vec::with_capacity(job.sections.len() + 2 * ranges.len());
    let mut m = 0;
    for s in job.sections.drain(..) {
        let (mut at, end) = (s.byte_range.start.0, s.byte_range.end.0);
        let mut first = true;
        while at < end {
            while ranges.get(m).is_some_and(|r| r.end <= at) {
                m += 1;
            }
            let (next, bg) = match ranges.get(m) {
                Some(r) if r.start <= at => (r.end.min(end), Some(if current == Some(m) { colors.current_bg } else { colors.bg })),
                Some(r) => (r.start.min(end), None),
                None => (end, None),
            };
            let mut piece = s.clone();
            piece.byte_range = ByteIndex(at)..ByteIndex(next);
            if !first {
                piece.leading_space = 0.0;
            }
            if let Some(bg) = bg {
                piece.format.background = bg;
                piece.format.color = colors.text;
            }
            out.push(piece);
            first = false;
            at = next;
        }
    }
    job.sections = out;
}

/// Highlights `query` in all text painted on the ui's layer from shape `from` on, whatever drew
/// it (Markdown, tables, code blocks), the `current`-th occurrence in its own colour, and
/// returns where that one is. Matched galleys are laid out again with the marks; that only
/// changes colours, so nothing moves.
pub fn highlight_painted(ui: &egui::Ui, from: usize, query: &str, current: Option<usize>, colors: Colors) -> Option<Rect> {
    if query.is_empty() {
        return None;
    }
    let layer = ui.layer_id();
    let mut texts = Vec::new();
    ui.ctx().graphics(|g| {
        for (i, c) in g.get(layer).into_iter().flat_map(|l| l.all_entries().enumerate().skip(from)) {
            collect_text(&c.shape, i, &mut Vec::new(), &mut texts);
        }
    });
    // Laid out outside the graphics lock (layout needs the context too).
    let (mut seen, mut target, mut marked) = (0, None, Vec::new());
    for (i, path, mut t) in texts {
        let text = &t.galley.job.text;
        let ranges: Vec<_> = find(&[text], query).into_iter().map(|m| m.range).collect();
        if ranges.is_empty() {
            continue;
        }
        let cur = current.and_then(|c| c.checked_sub(seen)).filter(|&c| c < ranges.len());
        if let Some(k) = cur {
            let at = t.galley.pos_from_cursor(CCursor::new(text[..ranges[k].start].chars().count()));
            target = Some(at.translate(t.pos.to_vec2()));
        }
        seen += ranges.len();
        let mut job = (*t.galley.job).clone();
        // An override colour would paint the marked text too; give it to the sections instead.
        if let Some(c) = t.override_text_color.take() {
            job.sections.iter_mut().for_each(|s| s.format.color = c);
        }
        mark(&mut job, &ranges, cur, colors);
        t.galley = ui.painter().layout_job(job);
        marked.push((i, path, t));
    }
    ui.ctx().graphics_mut(|g| {
        let list = g.entry(layer);
        for (i, path, t) in marked {
            list.mutate_shape(ShapeIdx(i), |c| {
                if let Some(s) = shape_at(&mut c.shape, &path) {
                    *s = Shape::Text(t);
                }
            });
        }
    });
    target
}

/// Text shapes in `shape` (nested ones by their path), tagged with the paint list index `i`.
fn collect_text(shape: &Shape, i: usize, path: &mut Vec<usize>, out: &mut Vec<(usize, Vec<usize>, TextShape)>) {
    match shape {
        Shape::Text(t) => out.push((i, path.clone(), t.clone())),
        Shape::Vec(v) => {
            for (j, s) in v.iter().enumerate() {
                path.push(j);
                collect_text(s, i, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

fn shape_at<'a>(shape: &'a mut Shape, path: &[usize]) -> Option<&'a mut Shape> {
    match (path.split_first(), shape) {
        (None, s) => Some(s),
        (Some((&j, rest)), Shape::Vec(v)) => v.get_mut(j).and_then(|s| shape_at(s, rest)),
        _ => None,
    }
}

/// Where the next shape painted on the ui's layer will go, for `highlight_painted`.
pub fn next_shape(ui: &egui::Ui) -> usize {
    ui.ctx().graphics(|g| g.get(ui.layer_id()).map_or(0, |l| l.next_idx().0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{FontId, TextFormat};

    #[test]
    fn finds_every_match_ignoring_case() {
        let m = find(&["Foo foo", "bar", "xFOOx"], "foo");
        let got: Vec<_> = m.iter().map(|m| (m.item, m.range.clone())).collect();
        assert_eq!(got, vec![(0, 0..3), (0, 4..7), (2, 1..4)]);
        assert!(find(&["anything"], "").is_empty());
    }

    #[test]
    fn matches_do_not_overlap_and_stay_on_char_boundaries() {
        assert_eq!(find(&["aaaa"], "aa").len(), 2);
        let text = "olá, Olá — ÁGUA";
        for m in find(&[text], "olá") {
            assert!(text.is_char_boundary(m.range.start) && text.is_char_boundary(m.range.end));
            assert_eq!(text[m.range].to_ascii_lowercase(), "olá");
        }
    }

    #[test]
    fn steps_wrap_around_and_new_queries_start_over() {
        let mut f = Find { query: "a".into(), ..Default::default() };
        f.update("main", &["a a", "a"]);
        assert_eq!((f.matches.len(), f.current, f.status()), (3, 0, "1 of 3".to_owned()));
        f.step(false);
        assert_eq!(f.current, 2);
        f.step(true);
        assert_eq!(f.current, 0);
        f.step(true);
        // The same query and texts keep the place; a new query or target starts over.
        f.update("main", &["a a", "a"]);
        assert_eq!(f.current, 1);
        f.query = "a a".into();
        f.update("main", &["a a", "a"]);
        assert_eq!((f.current, f.matches.len()), (0, 1));
        f.step(true);
        f.update("file", &["no match here"]);
        assert_eq!((f.current, f.status()), (0, "No results".to_owned()));
    }

    #[test]
    fn growing_text_keeps_the_current_match() {
        let mut f = Find { query: "x".into(), ..Default::default() };
        f.update("main", &["x", "x"]);
        f.step(true);
        f.update("main", &["x", "x", "more x"]);
        assert_eq!((f.current, f.matches.len()), (1, 3));
    }

    #[test]
    fn matches_in_one_item_know_the_current_one() {
        let mut f = Find { query: "b".into(), ..Default::default() };
        f.update("main", &["b", "abcb", "b"]);
        f.step(true);
        f.step(true);
        assert_eq!(f.in_item(1), (vec![1..2, 3..4], Some(1)));
        assert_eq!(f.in_item(0), (vec![0..1], None));
    }

    #[test]
    fn marks_split_sections_at_match_edges() {
        let mut job = LayoutJob::default();
        // Two colours, so the sections stay apart (equal ones would be merged).
        job.append("hello ", 0.0, TextFormat::simple(FontId::monospace(12.0), Color32::WHITE));
        job.append("world", 0.0, TextFormat::simple(FontId::monospace(12.0), Color32::GRAY));
        // "lo wo" spans both sections; "d" is the current match.
        let colors = Colors { bg: Color32::YELLOW, current_bg: Color32::RED, text: Color32::BLACK };
        mark(&mut job, &[3..8, 10..11], Some(1), colors);
        let got: Vec<_> = job.sections.iter().map(|s| (s.byte_range.start.0..s.byte_range.end.0, s.format.background)).collect();
        assert_eq!(
            got,
            vec![
                (0..3, Color32::TRANSPARENT),
                (3..6, Color32::YELLOW),
                (6..8, Color32::YELLOW),
                (8..10, Color32::TRANSPARENT),
                (10..11, Color32::RED),
            ]
        );
        // Marked text is drawn in the match text colour, the rest keeps its own.
        assert_eq!(job.sections[1].format.color, Color32::BLACK);
        assert_eq!(job.sections[0].format.color, Color32::WHITE);
    }
}
