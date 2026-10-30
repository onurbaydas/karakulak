//! Screen sharing (Meet, Zoom, Discord...): the wlr-screencopy protocol.
//! xdg-desktop-portal-wlr asks for frames here and hands them to Firefox
//! through PipeWire. Each frame is drawn once more into memory, like a
//! screenshot, and copied into the client's shared-memory buffer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::{
        Bind, ExportMem, Offscreen, Renderer, Texture, element::RenderElement, gles::GlesRenderbuffer,
    },
    output::Output,
    reexports::{
        wayland_protocols_wlr::screencopy::v1::server::{
            zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
            zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
        },
        wayland_server::{
            Client, DataInit, DisplayHandle, New, Resource,
            protocol::{wl_buffer::WlBuffer, wl_shm},
        },
    },
    utils::{Physical, Rectangle},
    wayland::{Dispatch2, GlobalDispatch2, shm},
};

use crate::State;

/// A frame waiting for damage is sent anyway after this long, so a still
/// screen still reaches the other side.
const MAX_WAIT: Duration = Duration::from_secs(1);

pub fn init(dh: &DisplayHandle) {
    dh.create_global::<State, ZwlrScreencopyManagerV1, _>(3, Manager);
}

/// A copy waiting for the next frame of its screen.
pub struct Pending {
    frame: ZwlrScreencopyFrameV1,
    buffer: WlBuffer,
    output: Output,
    region: Rectangle<i32, Physical>,
    /// `copy_with_damage`: wait until the screen changes.
    with_damage: bool,
    since: Instant,
}

struct Manager;

impl GlobalDispatch2<ZwlrScreencopyManagerV1, State> for Manager {
    fn bind(
        &self,
        _state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrScreencopyManagerV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, Manager);
    }
}

impl Dispatch2<ZwlrScreencopyManagerV1, State> for Manager {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        _resource: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        let (frame, output, region) = match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput { frame, output, .. } => (frame, output, None),
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                output,
                x,
                y,
                width,
                height,
                ..
            } => (frame, output, Some(Rectangle::new((x, y).into(), (width, height).into()))),
            _ => return,
        };
        let screen = Output::from_resource(&output).and_then(|output| {
            let size = output.current_mode()?.size;
            let full = Rectangle::from_size(size);
            let scale = output.current_scale().fractional_scale();
            let region = match region {
                Some(region) => region.to_f64().to_physical(scale).to_i32_round().intersection(full)?,
                None => full,
            };
            (!region.is_empty()).then_some((output, region))
        });
        let Some((output, region)) = screen else {
            data_init.init(frame, Frame::default()).failed();
            return;
        };
        let (w, h) = (region.size.w as u32, region.size.h as u32);
        let frame = data_init.init(
            frame,
            Frame {
                target: Some((output, region)),
                used: AtomicBool::new(false),
            },
        );
        frame.buffer(wl_shm::Format::Xrgb8888, w, h, w * 4);
        if frame.version() >= 3 {
            frame.buffer_done();
        }
    }
}

#[derive(Default)]
struct Frame {
    /// None for a frame that failed right away.
    target: Option<(Output, Rectangle<i32, Physical>)>,
    used: AtomicBool,
}

impl Dispatch2<ZwlrScreencopyFrameV1, State> for Frame {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        resource: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        let (buffer, with_damage) = match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => (buffer, false),
            zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => (buffer, true),
            _ => return,
        };
        let Some((output, region)) = self.target.clone() else {
            return;
        };
        if self.used.swap(true, Ordering::Relaxed) {
            resource.post_error(zwlr_screencopy_frame_v1::Error::AlreadyUsed, "frame already copied");
            return;
        }
        let fits = shm::with_buffer_contents(&buffer, |_, _, data| {
            data.format == wl_shm::Format::Xrgb8888
                && data.width == region.size.w
                && data.height == region.size.h
                && data.stride == region.size.w * 4
        });
        if !matches!(fits, Ok(true)) {
            resource.post_error(zwlr_screencopy_frame_v1::Error::InvalidBuffer, "wrong buffer");
            return;
        }
        state.screencopies.push(Pending {
            frame: resource.clone(),
            buffer,
            output,
            region,
            with_damage,
            since: Instant::now(),
        });
    }
}

/// Whether a frame of `output` has to be copied now; `damaged` says whether
/// the screen changed since the last frame.
pub fn wanted(pending: &mut Vec<Pending>, output: &Output, damaged: bool) -> bool {
    pending.retain(|p| p.frame.is_alive() && p.buffer.is_alive());
    pending
        .iter()
        .any(|p| p.output == *output && (damaged || !p.with_damage || p.since.elapsed() > MAX_WAIT))
}

/// Draws `elements` once more and fills the waiting buffers of `output`.
pub fn serve<R, E>(pending: &mut Vec<Pending>, renderer: &mut R, output: &Output, elements: &[E])
where
    R: Renderer + Offscreen<GlesRenderbuffer> + Bind<GlesRenderbuffer> + ExportMem,
    R::TextureId: Texture,
    E: RenderElement<R>,
{
    let (ready, waiting): (Vec<Pending>, Vec<Pending>) =
        std::mem::take(pending).into_iter().partition(|p| p.output == *output);
    *pending = waiting;
    let pixels = match crate::screenshot::render(renderer, output, elements) {
        Ok(pixels) => pixels,
        Err(e) => {
            tracing::warn!("screen sharing: {e}");
            ready.iter().for_each(|p| p.frame.failed());
            return;
        }
    };
    let (sec, nsec) = monotonic_now();
    for pending in ready {
        if !copy(&pixels, &pending) {
            pending.frame.failed();
            continue;
        }
        let frame = &pending.frame;
        if pending.with_damage {
            frame.damage(0, 0, pending.region.size.w as u32, pending.region.size.h as u32);
        }
        frame.flags(zwlr_screencopy_frame_v1::Flags::empty());
        frame.ready((sec >> 32) as u32, sec as u32, nsec);
    }
}

/// Copies the region of the RGBA picture into the client's XRGB buffer
/// (bytes B, G, R, X in memory).
fn copy((size, rgba): &(smithay::utils::Size<i32, Physical>, Vec<u8>), pending: &Pending) -> bool {
    let region = pending.region;
    let copied = shm::with_buffer_contents_mut(&pending.buffer, |ptr, len, data| {
        let row_bytes = region.size.w as usize * 4;
        let end = data.offset as usize + data.stride as usize * region.size.h as usize;
        if end > len || region.loc.x + region.size.w > size.w || region.loc.y + region.size.h > size.h {
            return false;
        }
        for row in 0..region.size.h as usize {
            let from = ((region.loc.y as usize + row) * size.w as usize + region.loc.x as usize) * 4;
            let source = &rgba[from..from + row_bytes];
            // Safety: in bounds of the pool (checked above); the client may
            // change the memory meanwhile, which only spoils its own picture.
            let target = unsafe {
                std::slice::from_raw_parts_mut(ptr.add(data.offset as usize + row * data.stride as usize), row_bytes)
            };
            for (to, from) in target.as_chunks_mut::<4>().0.iter_mut().zip(source.as_chunks::<4>().0) {
                *to = [from[2], from[1], from[0], 255];
            }
        }
        true
    });
    matches!(copied, Ok(true))
}

fn monotonic_now() -> (u64, u32) {
    let mut now: libc::timespec = unsafe { std::mem::zeroed() };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    (now.tv_sec as u64, now.tv_nsec as u32)
}
