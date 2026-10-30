//! Running on real screens: GPUs through DRM/KMS, input through libinput, and
//! the seat (permission to use the hardware, VT switching) through libseat.
//! Supports several GPUs, e.g. laptops where the built-in screen hangs off the
//! integrated GPU and the HDMI port off the discrete one.
//! Adapted from Smithay's anvil (MIT).

use std::{collections::HashMap, path::Path, time::Duration};

use smithay::{
    backend::{
        SwapBuffersError,
        allocator::{
            Fourcc, Modifier,
            dmabuf::Dmabuf,
            format::FormatSet,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{
            DrmDevice, DrmDeviceFd, DrmError, DrmEvent, DrmNode, NodeType,
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
        },
        egl::{EGLDevice, EGLDisplay},
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            ImportAll, ImportDma, ImportMem, ImportMemWl, Renderer,
            element::{
                AsRenderElements, memory::MemoryRenderBuffer, solid::SolidColorRenderElement,
                surface::WaylandSurfaceRenderElement,
            },
            gles::GlesRenderer,
            multigpu::{GpuManager, MultiRenderer, gbm::GbmGlesBackend},
        },
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, all_gpus, primary_gpu},
    },
    desktop::space::SpaceRenderElements,
    output::{Mode as WlMode, Output, PhysicalProperties},
    reexports::{
        calloop::{
            EventLoop, RegistrationToken,
            timer::{TimeoutAction, Timer},
        },
        drm::{
            Device as _,
            control::{Device as _, ModeTypeFlags, connector, crtc},
        },
        input::Libinput,
        rustix::fs::OFlags,
        wayland_server::{DisplayHandle, backend::GlobalId},
    },
    render_elements,
    utils::{DeviceFd, Logical, Point, Scale, Transform},
    wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
};
use smithay_drm_extras::{
    display_info,
    drm_scanner::{DrmScanEvent, DrmScanner},
};
use tracing::{error, info, warn};

use crate::{
    State,
    cursor::{Cursor, PointerElement, PointerRenderElement},
};

const SUPPORTED_FORMATS: &[Fourcc] = &[Fourcc::Abgr8888, Fourcc::Argb8888];

type UdevRenderer<'a> =
    MultiRenderer<'a, 'a, GbmGlesBackend<GlesRenderer, DrmDeviceFd>, GbmGlesBackend<GlesRenderer, DrmDeviceFd>>;
type Allocator = GbmAllocator<DrmDeviceFd>;
type Exporter = GbmFramebufferExporter<DrmDeviceFd>;

render_elements! {
    pub OutputElements<R, E> where R: ImportAll + ImportMem;
    Pointer=PointerRenderElement<R>,
    Dimmer=SolidColorRenderElement,
    Space=SpaceRenderElements<R, E>,
    Zoomed=smithay::backend::renderer::element::utils::RescaleRenderElement<SpaceRenderElements<R, E>>,
}

/// What a screen shows: pointer, dimming layer, windows (front to back).
type ScreenElements<'a> = OutputElements<UdevRenderer<'a>, WaylandSurfaceRenderElement<UdevRenderer<'a>>>;

impl<R: Renderer + ImportAll + ImportMem, E: smithay::backend::renderer::element::RenderElement<R> + std::fmt::Debug>
    std::fmt::Debug for OutputElements<R, E>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pointer(e) => f.debug_tuple("Pointer").field(e).finish(),
            Self::Dimmer(e) => f.debug_tuple("Dimmer").field(e).finish(),
            Self::Space(e) => f.debug_tuple("Space").field(e).finish(),
            Self::Zoomed(e) => f.debug_tuple("Zoomed").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

/// Which GPU and CRTC drive an output.
#[derive(Debug, PartialEq, Clone, Copy)]
struct OutputId {
    node: DrmNode,
    crtc: crtc::Handle,
}

pub struct UdevData {
    pub session: LibSeatSession,
    primary_gpu: DrmNode,
    gpus: GpuManager<GbmGlesBackend<GlesRenderer, DrmDeviceFd>>,
    backends: HashMap<DrmNode, Device>,
    dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
    cursor: Cursor,
    pointer_images: Vec<(xcursor::parser::Image, MemoryRenderBuffer)>,
    pointer_element: PointerElement,
}

/// One GPU.
struct Device {
    screens: HashMap<crtc::Handle, Screen>,
    drm_output_manager: DrmOutputManager<Allocator, Exporter, (), DrmDeviceFd>,
    drm_scanner: DrmScanner,
    render_node: Option<DrmNode>,
    registration_token: RegistrationToken,
}

/// One connected monitor.
struct Screen {
    dh: DisplayHandle,
    output: Output,
    global: Option<GlobalId>,
    drm_output: DrmOutput<Allocator, Exporter, (), DrmDeviceFd>,
    modes: Vec<smithay::reexports::drm::control::Mode>,
}

impl Drop for Screen {
    fn drop(&mut self) {
        self.output.leave_all();
        if let Some(global) = self.global.take() {
            self.dh.remove_global::<State>(global);
        }
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.udev.as_mut().unwrap().dmabuf_state.as_mut().unwrap().0
    }

    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        let udev = self.udev.as_mut().unwrap();
        let imported = udev
            .gpus
            .single_renderer(&udev.primary_gpu)
            .and_then(|mut renderer| renderer.import_dmabuf(&dmabuf, None))
            .is_ok();
        if imported {
            if dmabuf.node().is_none() {
                dmabuf.set_node(udev.primary_gpu);
            }
            let _ = notifier.successful::<State>();
        } else {
            notifier.failed();
        }
    }
}

pub fn init_udev(event_loop: &mut EventLoop<State>, state: &mut State) -> Result<(), Box<dyn std::error::Error>> {
    let (session, notifier) = LibSeatSession::new()?;
    let seat = session.seat();

    let primary_gpu = primary_gpu(&seat)?
        .and_then(|path| DrmNode::from_path(path).ok()?.node_with_type(NodeType::Render)?.ok())
        .or_else(|| {
            all_gpus(&seat)
                .ok()?
                .into_iter()
                .find_map(|path| DrmNode::from_path(path).ok())
        })
        .ok_or("no GPU found")?;
    info!("primary GPU: {primary_gpu}");

    let gpus = GpuManager::new(GbmGlesBackend::default())?;
    state.udev = Some(UdevData {
        session,
        primary_gpu,
        gpus,
        backends: HashMap::new(),
        dmabuf_state: None,
        cursor: Cursor::load(),
        pointer_images: Vec::new(),
        pointer_element: PointerElement::default(),
    });

    let udev_backend = UdevBackend::new(&seat)?;

    let mut libinput = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        state.udev.as_ref().unwrap().session.clone().into(),
    );
    libinput
        .udev_assign_seat(&seat)
        .map_err(|()| "libinput could not use the seat")?;
    event_loop
        .handle()
        .insert_source(LibinputInputBackend::new(libinput.clone()), |event, _, state| match event {
            InputEvent::DeviceAdded { mut device } => configure_touchpad(&mut device),
            InputEvent::DeviceRemoved { .. } => {}
            event => state.process_input_event(event),
        })?;

    // Switching to another VT pauses us; coming back resumes.
    event_loop.handle().insert_source(notifier, move |event, _, state| {
        let udev = state.udev.as_mut().unwrap();
        match event {
            SessionEvent::PauseSession => {
                libinput.suspend();
                for device in udev.backends.values_mut() {
                    device.drm_output_manager.pause();
                }
            }
            SessionEvent::ActivateSession => {
                if libinput.resume().is_err() {
                    error!("could not resume input");
                }
                let nodes: Vec<DrmNode> = udev.backends.keys().copied().collect();
                for device in udev.backends.values_mut() {
                    if let Err(e) = device.drm_output_manager.lock().activate(false) {
                        error!("could not resume GPU: {e}");
                    }
                }
                for node in nodes {
                    state.render_device(node);
                }
            }
        }
    })?;

    // The primary GPU first, so display-only GPUs can render through it.
    let primary_node = primary_gpu.node_with_type(NodeType::Primary).and_then(Result::ok);
    let mut devices: Vec<_> = udev_backend.device_list().map(|(id, path)| (id, path.to_owned())).collect();
    devices.sort_by_key(|(id, _)| Some(*id) != primary_node.map(|n| n.dev_id()) && *id != primary_gpu.dev_id());
    for (id, path) in devices {
        match DrmNode::from_dev_id(id) {
            Ok(node) => {
                if let Err(e) = state.device_added(node, &path) {
                    warn!("skipping GPU {path:?}: {e}");
                }
            }
            Err(e) => warn!("skipping GPU {path:?}: {e}"),
        }
    }

    let udev = state.udev.as_mut().unwrap();
    // Without a GPU driver the device renders in software under its card
    // node, not under the render node udev named as primary.
    if udev.gpus.single_renderer(&primary_gpu).is_err()
        && let Some(node) = udev.backends.values().find_map(|d| d.render_node)
    {
        info!("rendering on {node} instead of {primary_gpu}");
        udev.primary_gpu = node;
    }
    let primary_gpu = udev.primary_gpu;
    let renderer = udev.gpus.single_renderer(&primary_gpu)?;
    state.shm_state.update_formats(renderer.shm_formats());

    // Lets clients like Firefox hand us GPU buffers instead of copying pixels.
    let feedback = DmabufFeedbackBuilder::new(primary_gpu.dev_id(), renderer.dmabuf_formats()).build()?;
    let mut dmabuf_state = DmabufState::new();
    let global = dmabuf_state.create_global_with_default_feedback::<State>(&state.display_handle, &feedback);
    udev.dmabuf_state = Some((dmabuf_state, global));

    event_loop
        .handle()
        .insert_source(udev_backend, move |event, _, state| match event {
            UdevEvent::Added { device_id, path } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id)
                    && let Err(e) = state.device_added(node, &path)
                {
                    warn!("skipping GPU {path:?}: {e}");
                }
            }
            UdevEvent::Changed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    state.device_changed(node);
                }
            }
            UdevEvent::Removed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    state.device_removed(node);
                }
            }
        })?;

    Ok(())
}

impl State {
    fn device_added(&mut self, node: DrmNode, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let udev = self.udev.as_mut().unwrap();
        let fd = udev
            .session
            .open(path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)?;
        let fd = DrmDeviceFd::new(DeviceFd::from(fd));

        let (drm, notifier) = DrmDevice::new(fd.clone(), true)?;
        let gbm = GbmDevice::new(fd)?;

        let registration_token = self.loop_handle.insert_source(notifier, move |event, _, state: &mut State| match event {
            DrmEvent::VBlank(crtc) => state.frame_finish(node, crtc),
            DrmEvent::Error(e) => error!("GPU error: {e:?}"),
        })?;

        // Can this GPU render, or only drive displays?
        let render_node = (|| {
            let display = unsafe { EGLDisplay::new(gbm.clone()) }.ok()?;
            let egl_device = EGLDevice::device_for_display(&display).ok()?;
            // Software rendering (no GPU driver, e.g. a plain virtual machine)
            // is slow but better than a black screen; skip it only if a real
            // GPU is there to render for this device.
            if egl_device.is_software() {
                if udev.backends.values().any(|d| d.render_node.is_some()) {
                    return None;
                }
                warn!("no GPU acceleration on {node}, drawing in software");
            }
            let render_node = egl_device.try_get_render_node().ok().flatten().unwrap_or(node);
            udev.gpus.as_mut().add_node(render_node, gbm.clone()).ok()?;
            Some(render_node)
        })();

        let allocator = match render_node {
            Some(_) => GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT),
            None => udev
                .backends
                .values()
                .find(|device| device.render_node == Some(udev.primary_gpu))
                .map(|device| device.drm_output_manager.allocator().clone())
                .ok_or("GPU cannot render and the primary GPU is missing")?,
        };
        let exporter = GbmFramebufferExporter::new(gbm.clone(), render_node.into());

        let mut renderer = udev.gpus.single_renderer(&render_node.unwrap_or(udev.primary_gpu))?;
        let render_formats = renderer
            .as_mut()
            .egl_context()
            .dmabuf_render_formats()
            .iter()
            .filter(|format| render_node.is_some() || format.modifier == Modifier::Linear)
            .copied()
            .collect::<FormatSet>();

        let drm_output_manager = DrmOutputManager::new(
            drm,
            allocator,
            exporter,
            Some(gbm),
            SUPPORTED_FORMATS.iter().copied(),
            render_formats,
        );

        udev.backends.insert(
            node,
            Device {
                screens: HashMap::new(),
                drm_output_manager,
                drm_scanner: DrmScanner::new(),
                render_node,
                registration_token,
            },
        );
        self.device_changed(node);
        Ok(())
    }

    fn device_changed(&mut self, node: DrmNode) {
        let Some(device) = self.udev.as_mut().unwrap().backends.get_mut(&node) else {
            return;
        };
        let events = match device.drm_scanner.scan_connectors(device.drm_output_manager.device()) {
            Ok(events) => events,
            Err(e) => {
                warn!("could not scan connectors: {e:?}");
                return;
            }
        };
        for event in events {
            match event {
                DrmScanEvent::Connected {
                    connector,
                    crtc: Some(crtc),
                } => self.connector_connected(node, connector, crtc),
                DrmScanEvent::Disconnected {
                    connector,
                    crtc: Some(crtc),
                } => self.connector_disconnected(node, connector, crtc),
                _ => {}
            }
        }
        self.layout_outputs();
    }

    fn device_removed(&mut self, node: DrmNode) {
        let udev = self.udev.as_mut().unwrap();
        let Some(device) = udev.backends.get_mut(&node) else {
            return;
        };
        let crtcs: Vec<_> = device
            .drm_scanner
            .crtcs()
            .map(|(info, crtc)| (info.clone(), crtc))
            .collect();
        for (connector, crtc) in crtcs {
            self.connector_disconnected(node, connector, crtc);
        }
        let udev = self.udev.as_mut().unwrap();
        if let Some(device) = udev.backends.remove(&node) {
            if let Some(render_node) = device.render_node {
                udev.gpus.as_mut().remove_node(&render_node);
            }
            self.loop_handle.remove(device.registration_token);
        }
        self.layout_outputs();
    }

    fn connector_connected(&mut self, node: DrmNode, connector: connector::Info, crtc: crtc::Handle) {
        let udev = self.udev.as_mut().unwrap();
        let Some(device) = udev.backends.get_mut(&node) else {
            return;
        };
        let render_node = device.render_node.unwrap_or(udev.primary_gpu);
        let Ok(mut renderer) = udev.gpus.single_renderer(&render_node) else {
            return;
        };

        let name = format!("{}-{}", connector.interface().as_str(), connector.interface_id());
        let drm_device = device.drm_output_manager.device();
        let info = display_info::for_connector(drm_device, connector.handle());
        let make = info.as_ref().and_then(|i| i.make()).unwrap_or_else(|| "Unknown".into());
        let model = info.as_ref().and_then(|i| i.model()).unwrap_or_else(|| "Unknown".into());
        let serial_number = info.as_ref().and_then(|i| i.serial()).unwrap_or_else(|| "Unknown".into());

        let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));
        let output = Output::new(
            name.clone(),
            PhysicalProperties {
                size: (phys_w as i32, phys_h as i32).into(),
                subpixel: connector.subpixel().into(),
                make,
                model,
                serial_number,
            },
        );
        // The saved resolution if the monitor still offers it, else its preferred one.
        let (saved_mode, saved_scale) = crate::screens::saved(&output);
        let preferred = connector
            .modes()
            .iter()
            .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .or_else(|| connector.modes().first())
            .copied();
        let Some(preferred) = preferred else {
            return;
        };
        let drm_mode = saved_mode
            .and_then(|wanted| connector.modes().iter().find(|m| same_mode(m, wanted)).copied())
            .unwrap_or(preferred);
        let wl_mode = WlMode::from(drm_mode);
        let global = output.create_global::<State>(&self.display_handle);
        output.set_preferred(WlMode::from(preferred));
        let scale = saved_scale.map(smithay::output::Scale::Fractional);
        output.change_current_state(Some(wl_mode), None, scale, None);
        output.user_data().insert_if_missing(|| OutputId { node, crtc });

        // Overlay planes misbehave on NVIDIA's driver.
        let mut planes = drm_device.planes(&crtc).ok();
        let is_nvidia = drm_device
            .get_driver()
            .map(|d| d.name().to_string_lossy().to_lowercase().contains("nvidia"))
            .unwrap_or(false);
        if is_nvidia && let Some(planes) = planes.as_mut() {
            planes.overlay.clear();
        }

        let drm_output = match device
            .drm_output_manager
            .lock()
            .initialize_output::<_, ScreenElements<'_>>(
                crtc,
                drm_mode,
                &[connector.handle()],
                &output,
                planes,
                &mut renderer,
                &DrmOutputRenderElements::default(),
            ) {
            Ok(drm_output) => drm_output,
            Err(e) => {
                warn!("could not set up {name}: {e}");
                return;
            }
        };
        info!("screen connected: {name}");

        device.screens.insert(
            crtc,
            Screen {
                dh: self.display_handle.clone(),
                output: output.clone(),
                global: Some(global),
                drm_output,
                modes: connector.modes().to_vec(),
            },
        );
        // Placed for now; layout_outputs() arranges all screens afterwards.
        self.space.map_output(&output, (0, 0));
        self.loop_handle.insert_idle(move |state| state.render_screen(node, crtc));
    }

    fn connector_disconnected(&mut self, node: DrmNode, _connector: connector::Info, crtc: crtc::Handle) {
        let udev = self.udev.as_mut().unwrap();
        let Some(device) = udev.backends.get_mut(&node) else {
            return;
        };
        if let Some(screen) = device.screens.remove(&crtc) {
            info!("screen disconnected: {}", screen.output.name());
            self.space.unmap_output(&screen.output);
        }
    }

    /// Screens side by side, built-in screen first; then refit windows.
    pub fn layout_outputs(&mut self) {
        let mut outputs: Vec<Output> = self.space.outputs().cloned().collect();
        outputs.sort_by_key(|o| (!o.name().starts_with("eDP"), o.name()));
        let mut x = 0;
        for output in outputs {
            let scale = output.current_scale().fractional_scale();
            let width = output.current_mode().map_or(0, |m| m.size.to_f64().to_logical(scale).w.round() as i32);
            output.change_current_state(None, None, None, Some((x, 0).into()));
            self.space.map_output(&output, (x, 0));
            x += width;
        }
        self.refit_windows();
        self.fill_empty_screens();
    }

    fn render_device(&mut self, node: DrmNode) {
        let crtcs: Vec<crtc::Handle> = self
            .udev
            .as_ref()
            .and_then(|udev| udev.backends.get(&node))
            .map(|device| device.screens.keys().copied().collect())
            .unwrap_or_default();
        for crtc in crtcs {
            self.render_screen(node, crtc);
        }
    }

    fn frame_finish(&mut self, node: DrmNode, crtc: crtc::Handle) {
        let Some(screen) = self
            .udev
            .as_mut()
            .and_then(|udev| udev.backends.get_mut(&node))
            .and_then(|device| device.screens.get_mut(&crtc))
        else {
            return;
        };
        match screen.drm_output.frame_submitted() {
            Ok(_) => {
                self.loop_handle.insert_idle(move |state| state.render_screen(node, crtc));
            }
            Err(e) => warn!("frame error: {e:?}"),
        }
    }

    fn render_screen(&mut self, node: DrmNode, crtc: crtc::Handle) {
        let Some(output) = self
            .space
            .outputs()
            .find(|o| o.user_data().get::<OutputId>() == Some(&OutputId { node, crtc }))
            .cloned()
        else {
            return;
        };
        if !self.udev.as_ref().unwrap().session.is_active() || self.power.screens_off {
            return;
        }
        self.apply_gamma(&output, node, crtc);

        let overlays = self.overlays(&output);
        let pointer_location = self.seat.get_pointer().unwrap().current_location();
        let cursor_status = self.cursor_status.clone();
        let elapsed = self.start_time.elapsed();

        let udev = self.udev.as_mut().unwrap();
        let primary_gpu = udev.primary_gpu;
        let Some(device) = udev.backends.get_mut(&node) else {
            return;
        };
        let render_node = device.render_node.unwrap_or(primary_gpu);
        let Some(screen) = device.screens.get_mut(&crtc) else {
            return;
        };
        let renderer = if render_node == primary_gpu {
            udev.gpus.single_renderer(&render_node)
        } else {
            let format = screen.drm_output.format();
            udev.gpus.renderer(&primary_gpu, &render_node, format)
        };
        let mut renderer = match renderer {
            Ok(renderer) => renderer,
            Err(e) => {
                warn!("no renderer: {e}");
                return;
            }
        };

        let output_geometry = self.space.output_geometry(&output).unwrap();
        let scale = Scale::from(output.current_scale().fractional_scale());
        let mut elements: Vec<ScreenElements<'_>> = Vec::new();

        if output_geometry.to_f64().contains(pointer_location) {
            let image = udev.cursor.image(elapsed);
            let buffer = match udev.pointer_images.iter().find(|(i, _)| i == &image) {
                Some((_, buffer)) => buffer.clone(),
                None => {
                    let buffer = MemoryRenderBuffer::from_slice(
                        &image.pixels_rgba,
                        Fourcc::Argb8888,
                        (image.width as i32, image.height as i32),
                        1,
                        Transform::Normal,
                        None,
                    );
                    udev.pointer_images.push((image.clone(), buffer.clone()));
                    buffer
                }
            };
            udev.pointer_element.set_buffer(buffer);
            udev.pointer_element.set_status(cursor_status);
            let hotspot: Point<f64, Logical> = (image.xhot as f64, image.yhot as f64).into();
            let position = (pointer_location - output_geometry.loc.to_f64() - hotspot)
                .to_physical(scale)
                .to_i32_round();
            elements.extend(
                udev.pointer_element
                    .render_elements::<ScreenElements<'_>>(&mut renderer, position, scale, 1.0),
            );
        }
        elements.extend(overlays.into_iter().map(OutputElements::Dimmer));
        match self.space.render_elements_for_output(&mut renderer, &output, 1.0) {
            // The magnifier scales the windows around the pointer, which stays
            // put, so clicks still land where they appear to.
            Ok(space) if self.zoom > 1.0 => {
                let origin = (pointer_location - output_geometry.loc.to_f64()).to_physical(scale).to_i32_round();
                let zoom = self.zoom;
                elements.extend(space.into_iter().map(|e| {
                    OutputElements::Zoomed(smithay::backend::renderer::element::utils::RescaleRenderElement::from_element(e, origin, zoom))
                }));
            }
            Ok(space) => elements.extend(space.into_iter().map(OutputElements::Space)),
            Err(e) => warn!("could not render windows: {e:?}"),
        }

        if self.screenshots_pending.contains(&output.name()) {
            self.screenshots_pending.retain(|n| *n != output.name());
            match crate::screenshot::save(&mut renderer, &output, &elements) {
                Ok(path) => info!("screenshot saved: {}", path.display()),
                Err(e) => warn!("screenshot failed: {e}"),
            }
        }

        let rendered = screen
            .drm_output
            .render_frame(&mut renderer, &elements, [0.0, 0.0, 0.0, 1.0], FrameFlags::DEFAULT)
            .map(|result| !result.is_empty)
            .map_err(|e| warn!("render error: {e:?}"))
            .unwrap_or(false);
        if crate::screencopy::wanted(&mut self.screencopies, &output, rendered) {
            crate::screencopy::serve(&mut self.screencopies, &mut renderer, &output, &elements);
        }

        let queued = rendered
            && screen
                .drm_output
                .queue_frame(())
                .map_err(|e| {
                    let e = SwapBuffersError::from(e);
                    if !matches!(&e, SwapBuffersError::TemporaryFailure(err) if matches!(err.downcast_ref::<DrmError>(), Some(DrmError::DeviceInactive)))
                    {
                        warn!("could not show frame: {e:?}");
                    }
                })
                .is_ok();

        // Let clients draw their next frame.
        for window in self.space.elements_for_output(&output) {
            window.send_frame(&output, elapsed, Some(Duration::ZERO), |_, _| Some(output.clone()));
        }

        // Nothing changed: check again after about one frame.
        if !queued {
            let refresh = output.current_mode().map_or(60_000, |m| m.refresh.max(1));
            let delay = Duration::from_micros(1_000_000_000 / refresh as u64);
            self.loop_handle
                .insert_source(Timer::from_duration(delay), move |_, _, state| {
                    state.render_screen(node, crtc);
                    TimeoutAction::Drop
                })
                .ok();
        }
    }

    fn screen_for(&self, output: &Output) -> Option<(DrmNode, crtc::Handle)> {
        let id = *output.user_data().get::<OutputId>()?;
        Some((id.node, id.crtc))
    }

    pub fn udev_modes(&self, output: &Output) -> Vec<crate::screens::Mode> {
        let Some(id) = output.user_data().get::<OutputId>() else {
            return Vec::new();
        };
        let Some(screen) = self
            .udev
            .as_ref()
            .and_then(|u| u.backends.get(&id.node))
            .and_then(|d| d.screens.get(&id.crtc))
        else {
            return Vec::new();
        };
        let mut modes: Vec<crate::screens::Mode> = screen
            .modes
            .iter()
            .map(|m| {
                let wl = WlMode::from(*m);
                (wl.size.w, wl.size.h, wl.refresh)
            })
            .collect();
        modes.dedup();
        modes
    }

    pub fn udev_change_mode(&mut self, output: &Output, wanted: crate::screens::Mode) -> Result<(), String> {
        let (node, crtc) = self.screen_for(output).ok_or("not a real screen")?;
        let udev = self.udev.as_mut().unwrap();
        let primary_gpu = udev.primary_gpu;
        let device = udev.backends.get_mut(&node).ok_or("GPU gone")?;
        let render_node = device.render_node.unwrap_or(primary_gpu);
        let screen = device.screens.get_mut(&crtc).ok_or("screen gone")?;
        let mode = screen
            .modes
            .iter()
            .find(|m| same_mode(m, wanted))
            .copied()
            .ok_or("the monitor does not offer that resolution")?;
        let mut renderer = if render_node == primary_gpu {
            udev.gpus.single_renderer(&render_node)
        } else {
            let format = screen.drm_output.format();
            udev.gpus.renderer(&primary_gpu, &render_node, format)
        }
        .map_err(|e| e.to_string())?;
        screen
            .drm_output
            .use_mode::<_, ScreenElements<'_>>(mode, &mut renderer, &DrmOutputRenderElements::default())
            .map_err(|e| e.to_string())?;
        output.change_current_state(Some(WlMode::from(mode)), None, None, None);
        Ok(())
    }

    /// Writes the GPU gamma table when software brightness or night light
    /// changed. A GPU that refuses gets the black layer instead.
    fn apply_gamma(&mut self, output: &Output, node: DrmNode, crtc: crtc::Handle) {
        let wanted = self.software_state(output);
        let Some(screen) = self.brightness.screens.get_mut(&output.name()) else {
            return;
        };
        if !screen.gamma_works || screen.gamma_applied == Some(wanted) {
            return;
        }
        let Some(device) = self.udev.as_ref().and_then(|u| u.backends.get(&node)) else {
            return;
        };
        let drm = device.drm_output_manager.device();
        let size = drm.get_crtc(crtc).map(|c| c.gamma_length() as usize).unwrap_or(0);
        let [r, g, b] = crate::brightness::gamma_ramp(size, wanted.0, wanted.1);
        match (size > 1).then(|| drm.set_gamma(crtc, &r, &g, &b)) {
            Some(Ok(())) => screen.gamma_applied = Some(wanted),
            failed => {
                warn!("no gamma table on {} ({failed:?}), dimming with a layer", output.name());
                screen.gamma_works = false;
            }
        }
    }

    /// Powers all screens off (DPMS) after a long idle time.
    pub fn screens_off(&mut self) {
        let Some(udev) = self.udev.as_mut() else { return };
        for device in udev.backends.values_mut() {
            for screen in device.screens.values_mut() {
                if let Err(e) = screen.drm_output.with_compositor(|c| c.clear()) {
                    warn!("could not turn {} off: {e}", screen.output.name());
                }
            }
        }
    }

    /// Back on after input; the next frame re-enables each screen.
    pub fn screens_on(&mut self) {
        let Some(udev) = self.udev.as_mut() else { return };
        let mut nodes = Vec::new();
        for (node, device) in udev.backends.iter_mut() {
            for screen in device.screens.values_mut() {
                let _ = screen.drm_output.with_compositor(|c| c.reset_state());
            }
            nodes.push(*node);
        }
        // Gamma tables may have been reset with the screens.
        for screen in self.brightness.screens.values_mut() {
            screen.gamma_applied = None;
        }
        for node in nodes {
            self.render_device(node);
        }
    }

    /// Ctrl+Alt+F1..F12 switches to another virtual terminal.
    pub fn change_vt(&mut self, vt: i32) {
        if let Some(udev) = self.udev.as_mut()
            && let Err(e) = udev.session.change_vt(vt)
        {
            warn!("could not switch to VT {vt}: {e}");
        }
    }

    /// Buffers from a GPU other than the one drawing a screen are copied over
    /// early, when the client commits.
    pub fn early_import(&mut self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) {
        if let Some(udev) = self.udev.as_mut()
            && let Err(e) = udev.gpus.early_import(udev.primary_gpu, surface)
        {
            tracing::debug!("early import failed: {e}");
        }
    }
}

fn same_mode(mode: &smithay::reexports::drm::control::Mode, wanted: crate::screens::Mode) -> bool {
    let wl = WlMode::from(*mode);
    (wl.size.w, wl.size.h, wl.refresh) == wanted
}

/// Laptop touchpads: tap to click, natural scrolling, ignored while typing.
fn configure_touchpad(device: &mut smithay::reexports::input::Device) {
    if device.config_tap_finger_count() == 0 {
        return;
    }
    let _ = device.config_tap_set_enabled(true);
    let _ = device.config_scroll_set_natural_scroll_enabled(true);
    let _ = device.config_dwt_set_enabled(true);
}
