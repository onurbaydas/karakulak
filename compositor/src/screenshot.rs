//! Print Screen: every screen is drawn once more into memory and saved as a
//! PNG in the user's Pictures folder.

use std::path::PathBuf;

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, ExportMem, Offscreen, Renderer, Texture,
            damage::OutputDamageTracker,
            element::RenderElement,
            gles::GlesRenderbuffer,
        },
    },
    output::Output,
    utils::{Rectangle, Transform},
};

use crate::State;

impl State {
    /// Takes a picture of every screen on their next frame.
    pub fn request_screenshot(&mut self) {
        self.screenshots_pending = self.space.outputs().map(|o| o.name()).collect();
    }

    /// Whether this frame of `output` should be saved.
    pub fn screenshot_wanted(&mut self, output: &Output) -> bool {
        let before = self.screenshots_pending.len();
        self.screenshots_pending.retain(|name| *name != output.name());
        before != self.screenshots_pending.len()
    }
}

/// Renders `elements` (front to back) for `output` and saves them.
pub fn save<R, E>(renderer: &mut R, output: &Output, elements: &[E]) -> Result<PathBuf, String>
where
    R: Renderer + Offscreen<GlesRenderbuffer> + Bind<GlesRenderbuffer> + ExportMem,
    R::TextureId: Texture,
    E: RenderElement<R>,
{
    let err = |e: &dyn std::fmt::Display| e.to_string();
    let mode = output.current_mode().ok_or("screen has no mode")?;
    let size = mode.size;
    let buffer_size = size.to_logical(1).to_buffer(1, Transform::Normal);

    let mut target: GlesRenderbuffer = renderer.create_buffer(Fourcc::Abgr8888, buffer_size).map_err(|e| err(&e))?;
    let mut framebuffer = renderer.bind(&mut target).map_err(|e| err(&e))?;
    let mut tracker = OutputDamageTracker::new(size, output.current_scale().fractional_scale(), Transform::Normal);
    tracker
        .render_output(renderer, &mut framebuffer, 0, elements, [0.0, 0.0, 0.0, 1.0])
        .map_err(|e| format!("{e:?}"))?;
    let mapping = renderer
        .copy_framebuffer(&framebuffer, Rectangle::from_size(buffer_size), Fourcc::Abgr8888)
        .map_err(|e| err(&e))?;
    let pixels = renderer.map_texture(&mapping).map_err(|e| err(&e))?.to_vec();

    let path = file_name(output);
    let file = std::fs::File::create(&path).map_err(|e| err(&e))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size.w as u32, size.h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&pixels))
        .map_err(|e| err(&e))?;
    Ok(path)
}

/// ~/Resimler/Ekran görüntüsü 2026-10-30 14.05.33 eDP-1.png
fn file_name(output: &Output) -> PathBuf {
    let folder = dirs::picture_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&folder);
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    folder.join(format!(
        "Ekran görüntüsü {:04}-{:02}-{:02} {:02}.{:02}.{:02} {}.png",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        output.name()
    ))
}
