use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{FilterResult, keysyms},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, SERIAL_COUNTER},
};

/// Keys the compositor handles itself instead of passing to Firefox.
enum Action {
    /// Volume up/down by percent, or `None` to mute.
    Volume(Option<i32>),
    Brightness(f64),
    Screenshot,
    /// Magnifier: factor to multiply by, or `None` to switch it off.
    Zoom(Option<f64>),
    Tile(Slot),
    MoveToScreen(i32),
    NextWindow,
    /// Ctrl+Alt+F1..F12
    SwitchVt(i32),
    /// Ctrl+Alt+Backspace: leave, e.g. if something hangs during development.
    Quit,
}

use crate::{layout::Slot, state::State};

impl State {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        self.user_active();
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time(&event);

                let action = self.seat.get_keyboard().unwrap().input::<Action, _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, modifiers, handle| {
                        if event.state() != KeyState::Pressed {
                            return FilterResult::Forward;
                        }
                        let sym = handle.modified_sym().raw();
                        if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&sym) {
                            return FilterResult::Intercept(Action::SwitchVt(
                                (sym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32,
                            ));
                        }
                        if modifiers.ctrl && modifiers.alt && sym == keysyms::KEY_BackSpace {
                            return FilterResult::Intercept(Action::Quit);
                        }
                        let media = match sym {
                            keysyms::KEY_XF86AudioRaiseVolume => Some(Action::Volume(Some(5))),
                            keysyms::KEY_XF86AudioLowerVolume => Some(Action::Volume(Some(-5))),
                            keysyms::KEY_XF86AudioMute => Some(Action::Volume(None)),
                            keysyms::KEY_XF86MonBrightnessUp => Some(Action::Brightness(0.1)),
                            keysyms::KEY_XF86MonBrightnessDown => Some(Action::Brightness(-0.1)),
                            keysyms::KEY_Print => Some(Action::Screenshot),
                            _ => None,
                        };
                        if let Some(action) = media {
                            return FilterResult::Intercept(action);
                        }
                        if modifiers.alt && !modifiers.ctrl && sym == keysyms::KEY_Tab {
                            return FilterResult::Intercept(Action::NextWindow);
                        }
                        if modifiers.logo {
                            let action = match (modifiers.shift, sym) {
                                (false, keysyms::KEY_Left) => Some(Action::Tile(Slot::Left)),
                                (false, keysyms::KEY_Right) => Some(Action::Tile(Slot::Right)),
                                (false, keysyms::KEY_Up) => Some(Action::Tile(Slot::Full)),
                                (true, keysyms::KEY_Left) => Some(Action::MoveToScreen(-1)),
                                (true, keysyms::KEY_Right) => Some(Action::MoveToScreen(1)),
                                (_, keysyms::KEY_plus | keysyms::KEY_equal | keysyms::KEY_KP_Add) => Some(Action::Zoom(Some(1.25))),
                                (_, keysyms::KEY_minus | keysyms::KEY_KP_Subtract) => Some(Action::Zoom(Some(0.8))),
                                (_, keysyms::KEY_0 | keysyms::KEY_KP_0) => Some(Action::Zoom(None)),
                                _ => None,
                            };
                            if let Some(action) = action {
                                return FilterResult::Intercept(action);
                            }
                        }
                        FilterResult::Forward
                    },
                );
                match action {
                    Some(Action::SwitchVt(vt)) => self.change_vt(vt),
                    Some(Action::Quit) => self.loop_signal.stop(),
                    Some(Action::Volume(step)) => self.change_volume(step),
                    Some(Action::Brightness(step)) => self.change_brightness(step),
                    Some(Action::Screenshot) => self.request_screenshot(),
                    Some(Action::Zoom(factor)) => {
                        self.zoom = factor.map_or(1.0, |f| (self.zoom * f).clamp(1.0, 8.0));
                    }
                    Some(Action::Tile(slot)) => self.tile_focused(slot),
                    Some(Action::MoveToScreen(step)) => self.move_focused_to_screen(step),
                    Some(Action::NextWindow) => self.cycle_windows(),
                    None => {}
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let pos = self.clamp_to_screens(pointer.current_location() + event.delta());
                let under = self.surface_under(pos);
                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();

                let output_geo = self.space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                let under = self.surface_under(pos);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some((window, _loc)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        self.space.raise_element(&window, true);
                        keyboard.set_focus(
                            self,
                            Some(window.toplevel().unwrap().wl_surface().clone()),
                            serial,
                        );
                        self.space.elements().for_each(|window| {
                            window.toplevel().unwrap().send_pending_configure();
                        });
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activated(false);
                            window.toplevel().unwrap().send_pending_configure();
                        });
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                    }
                };

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event
                    .amount(Axis::Horizontal)
                    .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}

impl State {
    /// Keeps the pointer on some screen.
    fn clamp_to_screens(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        let Some(bounds) = self
            .space
            .outputs()
            .filter_map(|o| self.space.output_geometry(o))
            .reduce(|a, b| a.merge(b))
        else {
            return pos;
        };
        let x = pos.x.clamp(bounds.loc.x as f64, (bounds.loc.x + bounds.size.w - 1) as f64);
        let y = pos.y.clamp(bounds.loc.y as f64, (bounds.loc.y + bounds.size.h - 1) as f64);
        (x, y).into()
    }
}
