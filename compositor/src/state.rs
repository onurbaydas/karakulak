use std::{collections::HashMap, ffi::OsString, sync::Arc};

use serde_json::{Value, json};
use smithay::{
    backend::renderer::element::{Kind, solid::{SolidColorBuffer, SolidColorRenderElement}},
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    output::Output,
    input::{Seat, SeatState, pointer::CursorImageStatus},
    reexports::{
        calloop::{EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction, generic::Generic},
        wayland_server::{
            Display, DisplayHandle,
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
        },
    },
    utils::{Logical, Point, SERIAL_COUNTER},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

pub struct State {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,
    pub loop_handle: LoopHandle<'static, State>,
    /// Set when running on real screens (see udev.rs).
    pub udev: Option<crate::udev::UdevData>,
    pub cursor_status: CursorImageStatus,
    /// New-window requests sent to Firefox that have not shown up yet.
    pub windows_requested: usize,
    /// Windows that have not committed yet; we place them once we know
    /// their app id (lock window or not).
    pub pending: Vec<Window>,
    pub lock: crate::lock::Lock,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<State>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,

    /// Software brightness per output name: a black layer drawn over the
    /// screen. Works on every screen, unlike backlight or DDC/CI.
    pub dimmers: HashMap<String, (f64, SolidColorBuffer)>,
}

/// Darkest allowed level: a black screen looks like a dead machine.
const MIN_BRIGHTNESS: f64 = 0.1;

impl State {
    pub fn new(event_loop: &mut EventLoop<'static, Self>, display: Display<Self>) -> Self {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");
        let (layout, variant) = crate::keyboard::system_layout();
        let xkb = smithay::input::keyboard::XkbConfig {
            layout: &layout,
            variant: &variant,
            ..Default::default()
        };
        seat.add_keyboard(xkb, 200, 25)
            .or_else(|_| seat.add_keyboard(Default::default(), 200, 25))
            .unwrap();
        seat.add_pointer();

        let socket_name = Self::init_wayland_listener(display, event_loop);

        Self {
            start_time: std::time::Instant::now(),
            display_handle: dh,
            space: Space::default(),
            loop_signal: event_loop.get_signal(),
            loop_handle: event_loop.handle(),
            udev: None,
            cursor_status: CursorImageStatus::default_named(),
            windows_requested: 0,
            pending: Vec::new(),
            lock: Default::default(),
            socket_name,
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups: PopupManager::default(),
            seat,
            dimmers: HashMap::new(),
        }
    }

    fn init_wayland_listener(display: Display<State>, event_loop: &mut EventLoop<'static, Self>) -> OsString {
        let listening_socket = ListeningSocketSource::new_auto().unwrap();
        let socket_name = listening_socket.socket_name().to_os_string();
        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // Safety: we don't drop the display
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, (p + location).to_f64()))
        })
    }

    /// `[{"id", "name", "level"}]` for every screen.
    pub fn brightness_list(&self) -> Value {
        let screens: Vec<Value> = self
            .space
            .outputs()
            .map(|output| {
                let props = output.physical_properties();
                json!({
                    "id": output.name(),
                    "name": format!("{} {}", props.make, props.model),
                    "level": self.dimmers.get(&output.name()).map_or(1.0, |(level, _)| *level),
                })
            })
            .collect();
        Value::Array(screens)
    }

    pub fn set_brightness(&mut self, id: &str, level: f64) {
        let level = level.clamp(MIN_BRIGHTNESS, 1.0);
        let entry = self
            .dimmers
            .entry(id.to_string())
            .or_insert_with(|| (1.0, SolidColorBuffer::default()));
        entry.0 = level;
    }

    /// The dimming layer for `output`, if it is dimmed at all.
    pub fn dimmer(&mut self, output: &Output) -> Option<SolidColorRenderElement> {
        let size = self.space.output_geometry(output)?.size;
        let (level, buffer) = self.dimmers.get_mut(&output.name())?;
        if *level >= 1.0 {
            return None;
        }
        buffer.update(size, [0.0, 0.0, 0.0, (1.0 - *level) as f32]);
        Some(SolidColorRenderElement::from_buffer(
            buffer,
            (0, 0),
            output.current_scale().fractional_scale(),
            1.0,
            Kind::Unspecified,
        ))
    }

    /// Asks Firefox for a new window for every screen that has none, so each
    /// screen shows its own browser. Only once Firefox runs (it has a window);
    /// the command is KARAKULAK_NEW_WINDOW, default `firefox --new-window`.
    pub fn fill_empty_screens(&mut self) {
        let windows = self.space.elements().filter(|w| !is_floating(w)).count();
        let screens = self.space.outputs().count();
        if self.lock.locked || windows == 0 || self.windows_requested > 0 || windows >= screens {
            return;
        }
        let command = std::env::var("KARAKULAK_NEW_WINDOW").unwrap_or_else(|_| "firefox --new-window".into());
        let mut parts = command.split_whitespace();
        let Some(program) = parts.next() else {
            return;
        };
        let args: Vec<&str> = parts.collect();
        for _ in windows..screens {
            match std::process::Command::new(program).args(&args).spawn() {
                Ok(_) => self.windows_requested += 1,
                Err(e) => tracing::warn!("could not open a window for an empty screen: {e}"),
            }
        }
    }

    /// First commit of a new window: now its app id is known.
    pub fn map_new_window(&mut self, surface: &WlSurface) {
        let Some(index) = self
            .pending
            .iter()
            .position(|w| w.toplevel().unwrap().wl_surface() == surface)
        else {
            return;
        };
        let window = self.pending.remove(index);
        let toplevel = window.toplevel().unwrap().clone();

        if crate::lock::app_id(&window).as_deref() == Some(crate::lock::LOCK_APP_ID) {
            self.lock_window_created(window);
        } else {
            self.windows_requested = self.windows_requested.saturating_sub(1);
            if let Some(window) = self.hide_while_locked(window.clone()) {
                if is_floating(&window) {
                    self.place_floating(window);
                } else {
                    self.place(window);
                    self.fill_empty_screens();
                }
            } else if let Some(geometry) = self.primary_output().and_then(|o| self.space.output_geometry(&o)) {
                toplevel.with_pending_state(|state| state.size = Some(geometry.size));
            }
        }
        if !toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
    }

    /// Dialogs and the video popup keep their own size, centered on a screen.
    fn place_floating(&mut self, window: Window) {
        window.user_data().insert_if_missing(|| Floating(std::cell::Cell::new(false)));
        let Some(geometry) = self.primary_output().and_then(|o| self.space.output_geometry(&o)) else {
            return;
        };
        self.space.map_element(window.clone(), geometry.loc, true);
        self.center_floating(&window);
        let keyboard = self.seat.get_keyboard().unwrap();
        let surface = window.toplevel().unwrap().wl_surface().clone();
        keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
    }

    /// Centers a floating window once it knows its size.
    pub fn center_floating(&mut self, window: &Window) {
        let Some(floating) = window.user_data().get::<Floating>() else {
            return;
        };
        let size = window.geometry().size;
        if floating.0.get() || size.w == 0 {
            return;
        }
        let Some(screen) = self.primary_output().and_then(|o| self.space.output_geometry(&o)) else {
            return;
        };
        let offset: Point<i32, Logical> = ((screen.size.w - size.w) / 2, (screen.size.h - size.h) / 2).into();
        let loc = screen.loc + offset;
        self.space.map_element(window.clone(), loc, true);
        floating.0.set(true);
    }
}

/// Data associated with each connected client.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

/// Marks a floating window; the flag is set once it has been centered.
struct Floating(std::cell::Cell<bool>);

/// Windows that keep their own size instead of filling a screen: dialogs
/// (they have a parent), fixed-size windows, and Firefox's video popup.
pub fn is_floating(window: &Window) -> bool {
    if window.user_data().get::<Floating>().is_some() {
        return true;
    }
    let Some(toplevel) = window.toplevel() else {
        return false;
    };
    let surface = toplevel.wl_surface();
    smithay::wayland::compositor::with_states(surface, |states| {
        let (has_parent, title) = states
            .data_map
            .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok().map(|d| (d.parent.is_some(), d.title.clone())))
            .unwrap_or((false, None));
        let mut cached = states.cached_state.get::<smithay::wayland::shell::xdg::SurfaceCachedState>();
        let current = cached.current();
        let fixed = current.max_size.w > 0 && current.min_size == current.max_size;
        has_parent || fixed || title.as_deref() == Some("Picture-in-Picture")
    })
}
