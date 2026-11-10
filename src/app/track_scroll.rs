// SPDX-License-Identifier: GPL-3.0-or-later
use eframe::egui::*;
use std::time::Duration;

#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
struct Gesture {
    offset: f32,
    max_offset: f32,
    last_pos: Option<Pos2>,
    coast: f32,
    frame: u64,
}

pub(super) fn offset(ctx: &Context, id: Id) -> f32 {
    ctx.data(|d| d.get_temp::<Gesture>(id).map_or(0.0, |g| g.offset))
}

pub(super) fn moving(ctx: &Context, id: Id) -> bool {
    ctx.data(|d| {
        d.get_temp::<Gesture>(id).is_some_and(|g| {
            ctx.cumulative_frame_nr() <= g.frame + 1 && (g.last_pos.is_some() || g.coast != 0.0)
        })
    })
}

/// Move a list explicitly (for example, a live log's Latest action), cancelling
/// any fling. Ordinary touch scrolling still uses the same per-contact gesture.
pub(super) fn set_offset(ctx: &Context, id: Id, offset: f32) {
    ctx.data_mut(|d| {
        let mut gesture = d.get_temp::<Gesture>(id).unwrap_or_default();
        gesture.offset = offset.max(0.0);
        gesture.max_offset = gesture.max_offset.max(gesture.offset);
        gesture.coast = 0.0;
        gesture.last_pos = None;
        d.insert_temp(id, gesture);
    });
}

/// Register before the clickable rows, so rows own taps and the background owns
/// drags. Sense::click_and_drag waits for movement past egui's click threshold;
/// Sense::drag starts on touchdown, including movement from the previous touch.
pub(super) fn show_rows<R>(
    ui: &mut Ui,
    id: Id,
    height: f32,
    row_height: f32,
    count: usize,
    contents: impl FnOnce(&mut Ui, std::ops::Range<usize>) -> R,
) -> scroll_area::ScrollAreaOutput<R> {
    let rect = Rect::from_min_size(
        ui.available_rect_before_wrap().min,
        vec2(ui.available_width(), height),
    );
    let response = ui.interact(rect.intersect(ui.clip_rect()), id, Sense::click_and_drag());
    let frame = ui.ctx().cumulative_frame_nr();
    let mut gesture = ui
        .ctx()
        .data_mut(|d| d.get_temp::<Gesture>(id))
        .unwrap_or_default();
    // Do not resume an old fling after returning from Inspect or another list.
    if frame > gesture.frame + 1 {
        gesture.coast = 0.0;
        gesture.last_pos = None;
    }
    gesture.frame = frame;
    let (down, pressed, released, pos, origin, velocity, dt, cancel, wheel) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.any_pressed(),
            i.pointer.primary_released(),
            i.pointer.interact_pos(),
            i.pointer.press_origin(),
            i.pointer.velocity().y,
            i.stable_dt.min(0.1),
            i.raw.events.iter().any(|e| match e {
                Event::Touch {
                    phase: TouchPhase::Cancel,
                    ..
                }
                | Event::WindowFocused(false) => true,
                Event::PointerGone => i.pointer.primary_down(),
                _ => false,
            }),
            i.smooth_scroll_delta() != Vec2::ZERO,
        )
    });
    if wheel {
        gesture.coast = 0.0;
    }
    if pressed || cancel || !ui.is_enabled() {
        gesture.coast = 0.0;
        gesture.last_pos = None;
    }
    let mut delta = 0.0;
    if !cancel && response.dragged_by(PointerButton::Primary) {
        if let Some(pos) = pos {
            // The first delta starts at this contact's press origin, never at
            // the cursor position from the previous contact or mouse hover.
            let previous = gesture.last_pos.or(origin).unwrap_or(pos);
            delta = pos.y - previous.y;
            gesture.last_pos = Some(pos);
        }
        gesture.coast = 0.0;
    } else if !cancel && released && response.drag_stopped() && gesture.last_pos.is_some() {
        if let (Some(previous), Some(pos)) = (gesture.last_pos, pos) {
            delta = pos.y - previous.y;
        }
        // egui clears pointer velocity history on every new press.
        gesture.coast = velocity;
        gesture.last_pos = None;
    } else if down {
        gesture.coast = 0.0;
    } else {
        gesture.last_pos = None;
        let physics = ui.spacing().scroll.kinetic;
        if physics.decay_time > 0.0
            && gesture.coast.abs() * physics.decay_time > physics.stop_distance
        {
            let decay = (-dt / physics.decay_time).exp();
            delta = gesture.coast * physics.decay_time * (1.0 - decay);
            gesture.coast *= decay;
        } else {
            gesture.coast = 0.0;
        }
    }
    // Apply the movement before laying out rows. Updating at ScrollArea::end
    // would leave a pending movement that jumps the list on the next touchdown.
    gesture.offset = (gesture.offset - delta).clamp(0.0, gesture.max_offset);
    if (gesture.offset == 0.0 && gesture.coast > 0.0)
        || (gesture.offset == gesture.max_offset && gesture.coast < 0.0)
    {
        gesture.coast = 0.0;
    }
    let output = ScrollArea::vertical()
        .id_salt(id)
        .scroll_source(
            scroll_area::ScrollSource::SCROLL_BAR | scroll_area::ScrollSource::MOUSE_WHEEL,
        )
        .vertical_scroll_offset(gesture.offset)
        .auto_shrink([false, false])
        .max_height(height)
        .min_scrolled_height(height)
        .show_rows(ui, row_height, count, contents);
    // Keep wheel and scrollbar changes, and clamp against the current layout.
    gesture.offset = output.state.clamped_offset().y;
    gesture.max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
    if delta != 0.0 || gesture.coast != 0.0 {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, gesture));
    output
}
