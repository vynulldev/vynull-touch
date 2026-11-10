// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

struct List {
    ctx: Context,
    rows: Vec<(usize, Rect)>,
    selected: Option<usize>,
    offset: f32,
    viewport: Rect,
    visible: bool,
}
impl List {
    fn new() -> Self {
        let mut list = Self {
            ctx: Context::default(),
            rows: vec![],
            selected: None,
            offset: 0.0,
            viewport: Rect::NOTHING,
            visible: true,
        };
        crate::theme::configure(&list.ctx, 1.0);
        list.frame(vec![]);
        list.frame(vec![]);
        list
    }
    fn frame(&mut self, events: Vec<Event>) {
        let ctx = self.ctx.clone();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 480.0))),
            events,
            ..Default::default()
        };
        self.rows.clear();
        let mut output = ctx.run_ui(input, |root| {
            CentralPanel::default().show(root, |ui| {
                if self.visible {
                    let output =
                        show_rows(ui, Id::unique("test-list"), 200.0, 58.0, 96, |ui, range| {
                            for row in range {
                                let (rect, response) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 58.0),
                                    Sense::click(),
                                );
                                self.rows.push((row, rect));
                                if response.clicked() {
                                    self.selected = Some(row);
                                }
                            }
                        });
                    self.offset = output.state.clamped_offset().y;
                    self.viewport = output.inner_rect;
                }
            });
        });
        output.textures_delta.clear();
    }
    fn pointer(&mut self, pos: Pos2, phase: TouchPhase, touch: bool) {
        let mut events = vec![Event::PointerMoved(pos)];
        if matches!(phase, TouchPhase::Start | TouchPhase::End) {
            events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: phase == TouchPhase::Start,
                modifiers: Modifiers::NONE,
            });
        }
        if touch {
            events.push(Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(1),
                phase,
                pos,
                force: None,
            });
            if matches!(phase, TouchPhase::End | TouchPhase::Cancel) {
                events.push(Event::PointerGone);
            }
        }
        self.frame(events);
    }
    fn fling(&mut self, touch: bool) {
        let start = self.viewport.center_bottom() - vec2(0.0, 30.0);
        self.pointer(start, TouchPhase::Start, touch);
        for step in 1..=4 {
            self.pointer(
                start - vec2(0.0, step as f32 * 20.0),
                TouchPhase::Move,
                touch,
            );
        }
        self.pointer(start - vec2(0.0, 80.0), TouchPhase::End, touch);
    }
}

#[test]
fn small_finger_movements_remain_taps() {
    for touch in [false, true] {
        let mut list = List::new();
        let pos = list.rows[0].1.center();
        list.frame(vec![Event::PointerMoved(pos + vec2(0.0, 132.0))]);
        list.pointer(pos, TouchPhase::Start, touch);
        list.pointer(pos + vec2(2.0, -2.0), TouchPhase::Move, touch);
        for _ in 0..20 {
            list.frame(vec![]);
            assert_eq!(list.offset, 0.0);
        }
        list.pointer(pos, TouchPhase::End, touch);
        for _ in 0..20 {
            list.frame(vec![]);
            assert_eq!(list.offset, 0.0);
        }
        assert_eq!(list.selected, Some(0));
    }
}

#[test]
fn a_tap_stops_fling_at_the_displayed_position_and_selects_that_row() {
    for touch in [false, true] {
        let mut list = List::new();
        list.fling(touch);
        let released = list.offset;
        for _ in 0..5 {
            list.frame(vec![]);
        }
        assert!(
            list.offset > released + 10.0,
            "fling should coast after release: touch={touch}"
        );
        assert!(list.selected.is_none());
        let (row, rect) = *list
            .rows
            .iter()
            .find(|(_, r)| list.viewport.contains_rect(*r))
            .unwrap();
        let stopped = list.offset;
        list.pointer(rect.center(), TouchPhase::Start, touch);
        assert_eq!(
            list.offset, stopped,
            "touchdown must freeze the displayed list"
        );
        list.pointer(rect.center(), TouchPhase::End, touch);
        for _ in 0..30 {
            list.frame(vec![]);
            assert_eq!(list.offset, stopped);
        }
        assert_eq!(list.selected, Some(row));
    }
}

#[test]
fn cancellation_and_leaving_the_list_discard_scroll_momentum() {
    let mut list = List::new();
    let pos = list.rows[2].1.center();
    list.pointer(pos, TouchPhase::Start, true);
    list.pointer(pos - vec2(0.0, 40.0), TouchPhase::Move, true);
    let stopped = list.offset;
    list.pointer(pos - vec2(0.0, 40.0), TouchPhase::Cancel, true);
    list.pointer(pos - vec2(0.0, 40.0), TouchPhase::End, true);
    for _ in 0..20 {
        list.frame(vec![]);
        assert_eq!(list.offset, stopped);
    }
    assert!(list.selected.is_none());
    list.fling(true);
    let stopped = list.offset;
    list.visible = false;
    for _ in 0..3 {
        list.frame(vec![]);
    }
    list.visible = true;
    for _ in 0..20 {
        list.frame(vec![]);
        assert_eq!(list.offset, stopped);
    }
}

#[test]
fn mouse_wheel_and_scrollbar_still_move_the_list() {
    let mut list = List::new();
    list.frame(vec![
        Event::PointerMoved(list.viewport.center()),
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -100.0),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        },
    ]);
    for _ in 0..30 {
        list.frame(vec![]);
    }
    assert!(list.offset > 50.0);
    assert!(list.selected.is_none());
    let before = list.offset;
    // The floating scrollbar occupies the right edge of the viewport.
    let bar = list.viewport.right_top() + vec2(-3.0, 8.0);
    list.frame(vec![Event::PointerMoved(bar)]);
    list.pointer(bar, TouchPhase::Start, false);
    list.pointer(bar + vec2(0.0, 90.0), TouchPhase::Move, false);
    list.pointer(bar + vec2(0.0, 90.0), TouchPhase::End, false);
    for _ in 0..20 {
        list.frame(vec![]);
    }
    assert!(
        list.offset > before + 100.0,
        "scrollbar drag should move the thumb"
    );
    assert!(list.selected.is_none());
}
