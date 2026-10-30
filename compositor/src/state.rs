use std::{ffi::OsString, sync::Arc};

use smithay::{
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
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
    pub guest: crate::guest::Guest,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<State>,
    pub data_device_state: DataDeviceState,
    /// Select text, paste it with the middle button.
    pub primary_selection_state: smithay::wayland::selection::primary_selection::PrimarySelectionState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,
    /// Keyboard layouts as configured, e.g. ["tr", "us"].
    pub layouts: Vec<String>,

    pub brightness: crate::brightness::Brightness,
    pub overlays: crate::overlay::Overlays,
    pub power: crate::power::Power,
    /// Screens still to be saved for a Print Screen.
    pub screenshots_pending: Vec<String>,
    /// Screen sharing: copies waiting for the next frame.
    pub screencopies: Vec<crate::screencopy::Pending>,
    /// Magnifier: 1.0 is off.
    pub zoom: f64,
    pub idle_inhibit_state: smithay::wayland::idle_inhibit::IdleInhibitManagerState,
    /// Fractional scales (125%, 150%...) drawn sharply: clients render at the
    /// exact size and hand it over through a viewport.
    pub fractional_scale_state: smithay::wayland::fractional_scale::FractionalScaleManagerState,
    pub viewporter_state: smithay::wayland::viewporter::ViewporterState,
}

impl State {
    pub fn new(event_loop: &mut EventLoop<'static, Self>, display: Display<Self>) -> Self {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let primary_selection_state =
            smithay::wayland::selection::primary_selection::PrimarySelectionState::new::<Self>(&dh);

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

        let idle_inhibit_state = smithay::wayland::idle_inhibit::IdleInhibitManagerState::new::<Self>(&dh);
        let fractional_scale_state = smithay::wayland::fractional_scale::FractionalScaleManagerState::new::<Self>(&dh);
        let viewporter_state = smithay::wayland::viewporter::ViewporterState::new::<Self>(&dh);
        crate::screencopy::init(&dh);
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
            guest: Default::default(),
            socket_name,
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            primary_selection_state,
            popups: PopupManager::default(),
            seat,
            layouts: crate::keyboard::layout_names(&layout),
            brightness: Default::default(),
            overlays: Default::default(),
            power: Default::default(),
            screenshots_pending: Vec::new(),
            screencopies: Vec::new(),
            zoom: 1.0,
            idle_inhibit_state,
            fractional_scale_state,
            viewporter_state,
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

    /// Asks Firefox for a new window for every screen that has none, so each
    /// screen shows its own browser. Only once Firefox runs (it has a window);
    /// the command is KARAKULAK_NEW_WINDOW, default `firefox --new-window`.
    pub fn fill_empty_screens(&mut self) {
        let windows = self.space.elements().filter(|w| !is_floating(w)).count();
        let screens = self.space.outputs().count();
        if self.lock.locked || self.guest_active() || windows == 0 || self.windows_requested > 0 || windows >= screens {
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
            if self.guest_takes(&window) {
                if let Some(geometry) = self.primary_output().and_then(|o| self.space.output_geometry(&o)) {
                    toplevel.with_pending_state(|state| state.size = Some(geometry.size));
                }
            } else if let Some(window) = self.hide_while_locked(window.clone()) {
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
