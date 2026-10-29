use std::{collections::HashMap, ffi::OsString, sync::Arc};

use serde_json::{Value, json};
use smithay::{
    backend::renderer::element::{Kind, solid::{SolidColorBuffer, SolidColorRenderElement}},
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    reexports::{
        calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction, generic::Generic},
        wayland_protocols::xdg::shell::server::xdg_toplevel,
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
    pub fn new(event_loop: &mut EventLoop<Self>, display: Display<Self>) -> Self {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");
        seat.add_keyboard(Default::default(), 200, 25).unwrap();
        seat.add_pointer();

        let socket_name = Self::init_wayland_listener(display, event_loop);

        Self {
            start_time: std::time::Instant::now(),
            display_handle: dh,
            space: Space::default(),
            loop_signal: event_loop.get_signal(),
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

    fn init_wayland_listener(display: Display<State>, event_loop: &mut EventLoop<Self>) -> OsString {
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

    /// Makes `window` fill the first screen and gives it the keyboard.
    /// Windows are maximized, which keeps Firefox's tabs and toolbar; real
    /// fullscreen (hiding them) is only for when a page asks, e.g. a video.
    pub fn place(&mut self, window: Window) {
        let Some(output) = self.space.outputs().next().cloned() else {
            return;
        };
        let geometry = self.space.output_geometry(&output).unwrap();
        let toplevel = window.toplevel().unwrap();
        toplevel.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Maximized);
            state.states.set(xdg_toplevel::State::Activated);
            state.size = Some(geometry.size);
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
        self.space.map_element(window.clone(), geometry.loc, true);

        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, Some(toplevel.wl_surface().clone()), SERIAL_COUNTER.next_serial());
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
    pub fn dimmer(&mut self, output: &smithay::output::Output) -> Option<SolidColorRenderElement> {
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

    /// Refits every window after the screen changed size.
    pub fn refit_windows(&mut self) {
        let windows: Vec<Window> = self.space.elements().cloned().collect();
        for window in windows {
            self.place(window);
        }
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
