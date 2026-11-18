//! Selecting text by dragging past the top or bottom of a vertical scroll area scrolls it on,
//! faster the further out the pointer is, so a selection can run longer than the view.
//!
//! egui drops a label selection as soon as the label it started in is no longer drawn (it
//! must see both ends every frame). So while a selection is being dragged, the contents are
//! laid out with a clip rect that reaches back to where the drag started, and what that paints
//! is clipped to the view again afterwards.

use eframe::egui::{self, Rect, Ui, layers::ShapeIdx, text_selection::LabelSelectionState};

/// How close to the edge (inside it) the pointer starts the scrolling.
const EDGE: f32 = 6.0;

/// Drawn beyond each end of a selection, in case what is above it changes height.
const MARGIN: f32 = 300.0;

/// Started with [`begin`] first thing in a scroll area's contents, ended with [`DragScroll::end`]
/// last thing in them.
pub struct DragScroll {
    clip: Rect,
    from: usize,
    /// While selecting: how far the pointer is past the view's edge (negative above it).
    past: f32,
    /// While a selection lasts: the contents' y range (from their top) that must be drawn.
    keep: Option<(f32, f32)>,
    /// That selection was dragged across, not just clicked (which leaves an empty one).
    dragged: bool,
}

pub fn begin(ui: &mut Ui) -> DragScroll {
    let id = ui.id().with("drag-scroll");
    let clip = ui.clip_rect();
    let top = ui.max_rect().top();
    let from = ui.ctx().graphics(|g| g.get(ui.layer_id()).map_or(0, |l| l.next_idx().0));
    let mut ds = DragScroll { clip, from, past: 0.0, keep: None, dragged: false };
    let (pressed, down, origin, pointer) =
        ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.press_origin(), i.pointer.latest_pos()));
    // Where the drag started and where it is (or ended), from the contents' top, which moves
    // on screen as they scroll.
    if pressed {
        ui.data_mut(|d| match origin.filter(|p| clip.contains(*p)) {
            Some(p) => {
                d.insert_temp(id, (p.y - top, p.y - top));
            }
            None => {
                d.remove::<(f32, f32)>(id);
            }
        });
    }
    let Some((anchor, mut end)) = ui.data(|d| d.get_temp::<(f32, f32)>(id)) else { return ds };
    let selected = ui.ctx().with_plugin(|s: &mut LabelSelectionState| s.has_selection()).unwrap_or(false);
    if !down && !pressed && !selected {
        ui.data_mut(|d| d.remove::<(f32, f32)>(id));
        return ds;
    }
    if down && let Some(p) = pointer {
        end = p.y.clamp(clip.top(), clip.bottom()) - top;
        ui.data_mut(|d| d.insert_temp(id, (anchor, end)));
        if selected {
            ds.past = if p.y > clip.bottom() - EDGE {
                p.y - (clip.bottom() - EDGE)
            } else if p.y < clip.top() + EDGE {
                p.y - (clip.top() + EDGE)
            } else {
                0.0
            };
        }
    }
    // Both ends of the selection must be drawn for egui to keep it, even once out of view,
    // with room for the contents above them to change height a little (the conversation's
    // pinned prompt going inline).
    let (lo, hi) = (anchor.min(end) + top - MARGIN, anchor.max(end) + top + MARGIN);
    let span = Rect::from_x_y_ranges(clip.x_range(), lo.min(clip.top())..=hi.max(clip.bottom()));
    ui.set_clip_rect(span);
    ds.keep = Some((span.top() - top, span.bottom() - top));
    ds.dragged = selected && (end - anchor).abs() > 2.0;
    ds
}

impl DragScroll {
    /// The view: the contents' clip rect as it was before [`begin`] widened it.
    pub fn view(&self) -> Rect {
        self.clip
    }

    /// The contents' y range (from their top) to draw even if out of view, while a selection
    /// lasts: from where it starts to where it ends, and the view.
    pub fn keep(&self) -> Option<(f32, f32)> {
        self.keep
    }

    /// There is a selection made by dragging in these contents. Keep their layout as it is
    /// while it lasts: egui drops a selection whose ends move.
    pub fn selecting(&self) -> bool {
        self.dragged
    }

    /// Clips what was painted back to the view, and scrolls if the pointer is past its edge.
    pub fn end(self, ui: &mut Ui) {
        if self.keep.is_none() {
            return;
        }
        let layer = ui.layer_id();
        ui.ctx().graphics_mut(|g| {
            if let Some(list) = g.get_mut(layer) {
                for i in self.from..list.next_idx().0 {
                    list.mutate_shape(ShapeIdx(i), |s| s.clip_rect = s.clip_rect.intersect(self.clip));
                }
            }
        });
        ui.set_clip_rect(self.clip);
        if self.past != 0.0 {
            let dt = ui.input(|i| i.stable_dt).min(0.05);
            let speed = (self.past.abs() * 12.0).clamp(200.0, 5000.0) * self.past.signum();
            ui.scroll_with_delta_animation(egui::vec2(0.0, -speed * dt), egui::style::ScrollAnimation::none());
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dragging from the first line to below the view, and holding there, scrolls down, and
    /// the selection still reaches back to the first line.
    #[test]
    fn dragging_below_the_view_scrolls_and_keeps_the_selection() {
        let ctx = egui::Context::default();
        let lines: Vec<String> = (0..200).map(|i| format!("line {i}")).collect();
        let mut area = Rect::NOTHING;
        let mut offset = 0.0;
        let frame = |events: Vec<egui::Event>, area: &mut Rect, offset: &mut f32| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 600.0))),
                events,
                predicted_dt: 1.0 / 60.0,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let o = egui::ScrollArea::vertical().max_height(300.0).auto_shrink(false).show(ui, |ui| {
                    let drag = begin(ui);
                    for l in &lines {
                        ui.add(egui::Label::new(l).selectable(true));
                    }
                    drag.end(ui);
                });
                *area = o.inner_rect;
                *offset = o.state.offset.y;
            });
            out.textures_delta.clear();
            out
        };
        frame(vec![], &mut area, &mut offset);
        frame(vec![], &mut area, &mut offset);
        let start = area.left_top() + egui::vec2(0.0, 5.0);
        let below = egui::pos2(area.left() + 30.0, area.bottom() + 80.0);
        let press = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(vec![egui::Event::PointerMoved(start)], &mut area, &mut offset);
        frame(vec![press(start, true)], &mut area, &mut offset);
        for i in 1..=10 {
            let p = start + (below - start) * (i as f32 / 10.0);
            frame(vec![egui::Event::PointerMoved(p)], &mut area, &mut offset);
        }
        for _ in 0..60 {
            frame(vec![], &mut area, &mut offset);
        }
        assert!(offset > 300.0, "should have scrolled more than a page, scrolled {offset}");
        frame(vec![press(below, false)], &mut area, &mut offset);
        let out = frame(vec![egui::Event::Copy], &mut area, &mut offset);
        let copied: String = out.platform_output.commands.iter().filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        }).collect();
        assert!(copied.starts_with("line 0"), "selection lost its start: {copied:?}");
        // A page is about 15 lines: the selection runs well past the first one.
        assert!(copied.lines().count() > 25, "selection should run past the first page: {copied:?}");
    }
}
