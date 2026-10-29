//! The mouse pointer on real screens. Clients (Firefox) usually draw their
//! own cursor surface; otherwise the default cursor of the xcursor theme is
//! shown. Adapted from Smithay's anvil (MIT).

use std::{io::Read, time::Duration};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer, Texture,
        element::{
            AsRenderElements, Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
        },
    },
    input::pointer::CursorImageStatus,
    render_elements,
    utils::{Physical, Point, Scale},
};
use xcursor::{
    CursorTheme,
    parser::{Image, parse_xcursor},
};

static FALLBACK_CURSOR_DATA: &[u8] = include_bytes!("../resources/cursor.rgba");

pub struct Cursor {
    icons: Vec<Image>,
    size: u32,
}

impl Cursor {
    pub fn load() -> Cursor {
        let name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);

        let icons = load_icon(&CursorTheme::load(&name)).unwrap_or_else(|| {
            tracing::warn!("no xcursor theme, using the built-in cursor");
            vec![Image {
                size: 32,
                width: 64,
                height: 64,
                xhot: 1,
                yhot: 1,
                delay: 1,
                pixels_rgba: Vec::from(FALLBACK_CURSOR_DATA),
                pixels_argb: vec![],
            }]
        });
        Cursor { icons, size }
    }

    /// The frame to show now (themes may animate the cursor).
    pub fn image(&self, time: Duration) -> Image {
        let nearest = self
            .icons
            .iter()
            .min_by_key(|image| (self.size as i32 - image.size as i32).abs())
            .unwrap();
        let frames: Vec<&Image> = self
            .icons
            .iter()
            .filter(|image| image.width == nearest.width && image.height == nearest.height)
            .collect();
        let total: u32 = frames.iter().map(|image| image.delay).sum();
        if total == 0 {
            return frames[0].clone();
        }
        let mut millis = time.as_millis() as u32 % total;
        for image in &frames {
            if millis < image.delay {
                return (*image).clone();
            }
            millis -= image.delay;
        }
        frames[0].clone()
    }
}

fn load_icon(theme: &CursorTheme) -> Option<Vec<Image>> {
    let mut data = Vec::new();
    std::fs::File::open(theme.load_icon("default")?)
        .ok()?
        .read_to_end(&mut data)
        .ok()?;
    parse_xcursor(&data)
}

pub struct PointerElement {
    buffer: Option<MemoryRenderBuffer>,
    status: CursorImageStatus,
}

impl Default for PointerElement {
    fn default() -> Self {
        Self {
            buffer: None,
            status: CursorImageStatus::default_named(),
        }
    }
}

impl PointerElement {
    pub fn set_status(&mut self, status: CursorImageStatus) {
        self.status = status;
    }

    pub fn set_buffer(&mut self, buffer: MemoryRenderBuffer) {
        self.buffer = Some(buffer);
    }
}

render_elements! {
    pub PointerRenderElement<R> where R: ImportAll + ImportMem;
    Surface=WaylandSurfaceRenderElement<R>,
    Memory=MemoryRenderBufferRenderElement<R>,
}

impl<R: Renderer> std::fmt::Debug for PointerRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(e) => f.debug_tuple("Surface").field(e).finish(),
            Self::Memory(e) => f.debug_tuple("Memory").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

impl<T: Texture + Clone + Send + 'static, R> AsRenderElements<R> for PointerElement
where
    R: Renderer<TextureId = T> + ImportAll + ImportMem,
{
    type RenderElement = PointerRenderElement<R>;

    fn render_elements<E>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<E>
    where
        E: From<PointerRenderElement<R>>,
    {
        match &self.status {
            CursorImageStatus::Hidden => vec![],
            CursorImageStatus::Named(_) => self
                .buffer
                .as_ref()
                .and_then(|buffer| {
                    MemoryRenderBufferRenderElement::from_buffer(
                        renderer,
                        location.to_f64(),
                        buffer,
                        None,
                        None,
                        None,
                        Kind::Cursor,
                    )
                    .ok()
                })
                .map(|element| vec![PointerRenderElement::<R>::from(element).into()])
                .unwrap_or_default(),
            CursorImageStatus::Surface(surface) => {
                let elements: Vec<PointerRenderElement<R>> =
                    render_elements_from_surface_tree(renderer, surface, location, scale, alpha, Kind::Cursor);
                elements.into_iter().map(E::from).collect()
            }
        }
    }
}
