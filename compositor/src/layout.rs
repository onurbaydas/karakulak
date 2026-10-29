//! Where windows go. Each window sits on one screen, either filling it or on
//! its left or right half. New windows go to a screen that has none yet.
//!
//!   Super+←/→/↑        left half, right half, whole screen
//!   Super+Shift+←/→    move to the previous/next screen
//!   Alt+Tab            next window on this screen
//!   drag by the tab bar: drop at the left/right edge for a half, at the top
//!   for the whole screen, anywhere else to just change screens

use std::cell::RefCell;

use smithay::{
    desktop::Window,
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    output::Output,
    reexports::{wayland_protocols::xdg::shell::server::xdg_toplevel, wayland_server::protocol::wl_surface::WlSurface},
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};

use crate::{State, state::is_floating};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
    Full,
    Left,
    Right,
}

#[derive(Clone)]
struct Placement {
    output: String,
    slot: Slot,
}

/// How close to a screen edge a dragged window has to be dropped to snap.
const SNAP_EDGE: f64 = 48.0;

fn placement(window: &Window) -> Option<Placement> {
    window
        .user_data()
        .get::<RefCell<Option<Placement>>>()
        .and_then(|p| p.borrow().clone())
}

fn set_placement(window: &Window, placement: Placement) {
    window.user_data().insert_if_missing(|| RefCell::new(None::<Placement>));
    *window.user_data().get::<RefCell<Option<Placement>>>().unwrap().borrow_mut() = Some(placement);
}

fn area(screen: Rectangle<i32, Logical>, slot: Slot) -> Rectangle<i32, Logical> {
    let half = screen.size.w / 2;
    match slot {
        Slot::Full => screen,
        Slot::Left => Rectangle::new(screen.loc, (half, screen.size.h).into()),
        Slot::Right => Rectangle::new(
            (screen.loc.x + half, screen.loc.y).into(),
            (screen.size.w - half, screen.size.h).into(),
        ),
    }
}

impl State {
    /// Puts `window` where it was (if that screen still exists), else on a
    /// screen without windows, else on the first screen; gives it the keyboard.
    pub fn place(&mut self, window: Window) {
        let slot = placement(&window).map_or(Slot::Full, |p| p.slot);
        if let Some(output) = self.output_for(&window) {
            self.place_on(window, output, slot);
        }
    }

    /// The leftmost screen: the built-in one on laptops.
    pub fn primary_output(&self) -> Option<Output> {
        self.space
            .outputs()
            .min_by_key(|o| self.space.output_geometry(o).map_or(i32::MAX, |g| g.loc.x))
            .cloned()
    }

    pub fn place_on_primary(&mut self, window: Window) {
        if let Some(output) = self.primary_output() {
            self.place_on(window, output, Slot::Full);
        }
    }

    fn place_on(&mut self, window: Window, output: Output, slot: Slot) {
        set_placement(&window, Placement { output: output.name(), slot });
        let Some(rect) = self.space.output_geometry(&output).map(|g| area(g, slot)) else {
            return;
        };
        let toplevel = window.toplevel().unwrap();
        toplevel.with_pending_state(|state| {
            use xdg_toplevel::State as S;
            for s in [S::Maximized, S::TiledLeft, S::TiledRight, S::TiledTop, S::TiledBottom] {
                state.states.unset(s);
            }
            // Maximized keeps Firefox's toolbar without window borders; tiled
            // tells it a side is against another window.
            let set: &[S] = match slot {
                Slot::Full => &[S::Maximized],
                Slot::Left => &[S::TiledLeft, S::TiledTop, S::TiledBottom],
                Slot::Right => &[S::TiledRight, S::TiledTop, S::TiledBottom],
            };
            for s in set {
                state.states.set(*s);
            }
            state.states.set(S::Activated);
            state.size = Some(rect.size);
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
        self.space.map_element(window.clone(), rect.loc, true);

        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, Some(toplevel.wl_surface().clone()), SERIAL_COUNTER.next_serial());
    }

    fn output_for(&self, window: &Window) -> Option<Output> {
        let assigned = placement(window).map(|p| p.output);
        let outputs: Vec<&Output> = self.space.outputs().collect();
        if let Some(output) = outputs.iter().find(|o| Some(o.name()) == assigned) {
            return Some((*output).clone());
        }
        let used: Vec<String> = self
            .space
            .elements()
            .filter(|w| *w != window && !is_floating(w))
            .filter_map(|w| placement(w).map(|p| p.output))
            .collect();
        outputs
            .iter()
            .find(|o| !used.contains(&o.name()))
            .or(outputs.first())
            .map(|o| (*o).clone())
    }

    /// Refits every window after a screen changed size or went away.
    pub fn refit_windows(&mut self) {
        let windows: Vec<Window> = self.space.elements().filter(|w| !is_floating(w)).cloned().collect();
        for window in windows {
            if crate::lock::app_id(&window).as_deref() == Some(crate::lock::LOCK_APP_ID) {
                self.place_on_primary(window);
            } else {
                self.place(window);
            }
        }
    }

    /// The window with the keyboard, if it is a tiled one.
    fn focused_window(&self) -> Option<Window> {
        let focus = self.seat.get_keyboard()?.current_focus()?;
        self.space
            .elements()
            .find(|w| w.toplevel().is_some_and(|t| *t.wl_surface() == focus) && !is_floating(w))
            .cloned()
    }

    fn output_named(&self, name: &str) -> Option<Output> {
        self.space.outputs().find(|o| o.name() == name).cloned()
    }

    /// Screens left to right.
    fn outputs_in_order(&self) -> Vec<Output> {
        let mut outputs: Vec<Output> = self.space.outputs().cloned().collect();
        outputs.sort_by_key(|o| self.space.output_geometry(o).map_or(0, |g| g.loc.x));
        outputs
    }

    pub fn tile_focused(&mut self, slot: Slot) {
        let Some(window) = self.focused_window() else { return };
        let Some(output) = placement(&window).and_then(|p| self.output_named(&p.output)) else {
            return;
        };
        self.place_on(window, output, slot);
    }

    pub fn move_focused_to_screen(&mut self, step: i32) {
        let Some(window) = self.focused_window() else { return };
        let Some(current) = placement(&window) else { return };
        let outputs = self.outputs_in_order();
        let Some(index) = outputs.iter().position(|o| o.name() == current.output) else {
            return;
        };
        let target = (index as i32 + step).clamp(0, outputs.len() as i32 - 1) as usize;
        self.place_on(window, outputs[target].clone(), current.slot);
    }

    /// Brings the next window on the focused screen to the front.
    pub fn cycle_windows(&mut self) {
        let Some(focused) = self.focused_window() else { return };
        let Some(output) = placement(&focused).map(|p| p.output) else { return };
        let windows: Vec<Window> = self
            .space
            .elements()
            .filter(|w| !is_floating(w) && placement(w).is_some_and(|p| p.output == output))
            .cloned()
            .collect();
        if windows.len() < 2 {
            return;
        }
        // The space lists windows bottom to top: the lowest comes up next.
        let next = windows[0].clone();
        self.place(next);
    }

    /// A window was dropped after dragging it by its tab bar.
    fn drop_window(&mut self, window: Window, at: Point<f64, Logical>) {
        let Some(output) = self.space.output_under(at).next().cloned() else {
            self.place(window);
            return;
        };
        let screen = self.space.output_geometry(&output).unwrap().to_f64();
        let slot = if at.x < screen.loc.x + SNAP_EDGE {
            Slot::Left
        } else if at.x > screen.loc.x + screen.size.w - SNAP_EDGE {
            Slot::Right
        } else if at.y < screen.loc.y + SNAP_EDGE {
            Slot::Full
        } else {
            placement(&window).map_or(Slot::Full, |p| p.slot)
        };
        self.place_on(window, output, slot);
    }

    /// Firefox asked to move a window (drag on the tab bar).
    pub fn start_drag(&mut self, window: Window, serial: smithay::utils::Serial) {
        let pointer = self.seat.get_pointer().unwrap();
        if !pointer.has_grab(serial) {
            return;
        }
        let Some(start_data) = pointer.grab_start_data() else { return };
        let Some(origin) = self.space.element_location(&window) else { return };
        let grab = DragGrab { start_data, window, origin };
        pointer.set_grab(self, grab, serial, smithay::input::pointer::Focus::Clear);
    }
}

/// Moves the window with the pointer, then snaps it where it is dropped.
struct DragGrab {
    start_data: GrabStartData<State>,
    window: Window,
    origin: Point<i32, Logical>,
}

impl PointerGrab<State> for DragGrab {
    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
        let location = self.origin.to_f64() + (event.location - self.start_data.location);
        data.space.map_element(self.window.clone(), location.to_i32_round(), true);
    }

    fn relative_motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(&mut self, data: &mut State, handle: &mut PointerInnerHandle<'_, State>, event: &ButtonEvent) {
        handle.button(data, event);
        const BTN_LEFT: u32 = 0x110;
        if !handle.current_pressed().contains(&BTN_LEFT) {
            handle.unset_grab(self, data, event.serial, event.time, true);
            let at = handle.current_location();
            data.drop_window(self.window.clone(), at);
        }
    }

    fn axis(&mut self, data: &mut State, handle: &mut PointerInnerHandle<'_, State>, details: AxisFrame) {
        handle.axis(data, details)
    }

    fn frame(&mut self, data: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(&mut self, d: &mut State, h: &mut PointerInnerHandle<'_, State>, e: &GestureSwipeBeginEvent) {
        h.gesture_swipe_begin(d, e)
    }

    fn gesture_swipe_update(&mut self, d: &mut State, h: &mut PointerInnerHandle<'_, State>, e: &GestureSwipeUpdateEvent) {
        h.gesture_swipe_update(d, e)
    }

    fn gesture_swipe_end(&mut self, d: &mut State, h: &mut PointerInnerHandle<'_, State>, e: &GestureSwipeEndEvent) {
        h.gesture_swipe_end(d, e)
    }

    fn gesture_pinch_begin(&mut self, d: &mut State, h: &mut PointerInnerHandle<'_, State>, e: &GesturePinchBeginEvent) {
        h.gesture_pinch_begin(d, e)
    }

    fn gesture_pinch_update(&mut self, d: &mut State, h: &mut PointerInnerHandle<'_, State>, e: &GesturePinchUpdateEvent) {
        h.gesture_pinch_update(d, e)
    }

    fn gesture_pinch_end(&mut self, d: &mut State, h: &mut PointerInnerHandle<'_, State>, e: &GesturePinchEndEvent) {
        h.gesture_pinch_end(d, e)
    }

    fn gesture_hold_begin(&mut self, d: &mut State, h: &mut PointerInnerHandle<'_, State>, e: &GestureHoldBeginEvent) {
        h.gesture_hold_begin(d, e)
    }

    fn gesture_hold_end(&mut self, d: &mut State, h: &mut PointerInnerHandle<'_, State>, e: &GestureHoldEndEvent) {
        h.gesture_hold_end(d, e)
    }

    fn start_data(&self) -> &GrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut State) {}
}
