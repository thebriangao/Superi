//! Deterministic CPU preparation for the shared wgpu compositor.

use swash::scale::{image::Image, Render, ScaleContext, Source};
use swash::shape::{Direction, ShapeContext};
use swash::text::Script;
use swash::zeno::Format;
use swash::FontRef;

use crate::icons::{IconPrimitive, IconRegistry};
use crate::scene::{Color, NodeKind, Rect, Scene};
use crate::{Result, UiError};

const INTER_FONT: &[u8] = include_bytes!("../assets/InterVariable.ttf");

/// Exact prepared RGBA pixels before the product wgpu compositor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RasterFrame {
    /// Returns physical width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns physical height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns tightly packed RGBA8 pixels.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Creates a frame from checked tightly packed RGBA bytes.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| usize::try_from(height).ok().map(|height| width * height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| UiError::Invalid("raster dimensions are exhausted".to_owned()))?;
        if pixels.len() != expected {
            return Err(UiError::Invalid(format!(
                "raster byte count {} does not match {expected}",
                pixels.len()
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
}

/// Deterministic text, icon, seam, and plane preparation.
pub struct CpuPainter {
    icons: IconRegistry,
    shape_context: ShapeContext,
    scale_context: ScaleContext,
}

impl CpuPainter {
    /// Creates the pinned foundation painter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            icons: IconRegistry::foundation(),
            shape_context: ShapeContext::with_max_entries(16),
            scale_context: ScaleContext::with_max_entries(16),
        }
    }

    /// Paints retained nodes in deterministic z and insertion order.
    pub fn paint(mut self, scene: &Scene) -> Result<RasterFrame> {
        self.icons.validate()?;
        let width = scene.physical_width()?;
        let height = scene.physical_height()?;
        let byte_count = usize::try_from(width)
            .ok()
            .and_then(|width| usize::try_from(height).ok().map(|height| width * height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| UiError::Invalid("raster allocation is exhausted".to_owned()))?;
        let mut frame = RasterFrame {
            width,
            height,
            pixels: vec![0; byte_count],
        };
        let mut ordered = scene.nodes().iter().enumerate().collect::<Vec<_>>();
        ordered.sort_by_key(|(index, node)| (node.z(), *index));
        for (_, node) in ordered {
            if !node.visible() {
                continue;
            }
            let clip = node
                .clip()
                .and_then(|clip| clip.intersection(node.bounds()));
            match node.kind() {
                NodeKind::Rect { fill } => {
                    fill_rect(&mut frame, node.bounds(), *fill, scene.scale_factor(), clip);
                }
                NodeKind::Stroke { color, width } => {
                    draw_stroke(
                        &mut frame,
                        node.bounds(),
                        *width,
                        *color,
                        scene.scale_factor(),
                        clip,
                    );
                }
                NodeKind::Text {
                    text,
                    size,
                    weight,
                    color,
                } => {
                    self.draw_text(
                        &mut frame,
                        node.bounds(),
                        clip,
                        text,
                        *size,
                        *weight,
                        *color,
                        scene.scale_factor(),
                    )?;
                }
                NodeKind::Icon { name, color } => {
                    self.draw_icon(
                        &mut frame,
                        node.bounds(),
                        clip,
                        name,
                        *color,
                        scene.scale_factor(),
                    )?;
                }
            }
        }
        if let Some(focused) = scene.focused().and_then(|id| scene.node(id)) {
            draw_stroke(
                &mut frame,
                focused.hit_bounds(),
                2.0,
                Color::CYAN,
                scene.scale_factor(),
                focused.clip(),
            );
            let corner = Rect {
                x: focused.hit_bounds().x + 2.0,
                y: focused.hit_bounds().y + 2.0,
                width: 3.0,
                height: 3.0,
            };
            fill_rect(
                &mut frame,
                corner,
                Color::WHITE,
                scene.scale_factor(),
                focused.clip(),
            );
        }
        Ok(frame)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        frame: &mut RasterFrame,
        bounds: Rect,
        clip: Option<Rect>,
        text: &str,
        size: f32,
        weight: u16,
        color: Color,
        scale: f32,
    ) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let font = FontRef::from_index(INTER_FONT, 0)
            .ok_or_else(|| UiError::Unavailable("bundled Inter 4.1 is invalid".to_owned()))?;
        let physical_size = size * scale;
        let mut shaper = self
            .shape_context
            .builder(font)
            .script(Script::Latin)
            .direction(Direction::LeftToRight)
            .size(physical_size)
            .variations([("wght", f32::from(weight))])
            .build();
        let metrics = shaper.metrics();
        shaper.add_str(text);
        let mut glyphs = Vec::new();
        let mut advance = 0.0_f32;
        shaper.shape_with(|cluster| {
            for glyph in cluster.glyphs {
                glyphs.push((glyph.id, advance + glyph.x, glyph.y));
                advance += glyph.advance;
            }
        });
        let mut scaler = self
            .scale_context
            .builder(font)
            .size(physical_size)
            .hint(true)
            .variations([("wght", f32::from(weight))])
            .build();
        let baseline = bounds.y * scale + metrics.ascent;
        let origin_x = bounds.x * scale;
        let logical_clip = clip
            .and_then(|clip| clip.intersection(bounds))
            .unwrap_or(bounds);
        let pixel_clip = physical_rect(logical_clip, scale, frame.width, frame.height);
        let mut image = Image::new();
        for (glyph_id, glyph_x, glyph_y) in glyphs {
            image.clear();
            if !Render::new(&[Source::Outline])
                .format(Format::Alpha)
                .render_into(&mut scaler, glyph_id, &mut image)
            {
                continue;
            }
            let left = (origin_x + glyph_x + image.placement.left as f32).round() as i32;
            let top = (baseline - glyph_y - image.placement.top as f32).round() as i32;
            draw_mask(
                frame,
                (left, top),
                (image.placement.width, image.placement.height),
                &image.data,
                color,
                pixel_clip,
            );
        }
        Ok(())
    }

    fn draw_icon(
        &self,
        frame: &mut RasterFrame,
        bounds: Rect,
        clip: Option<Rect>,
        name: &str,
        color: Color,
        scale: f32,
    ) -> Result<()> {
        let definition = self.icons.get(name).ok_or_else(|| {
            UiError::Unavailable(format!("icon `{name}` is absent from registry 1.0.0"))
        })?;
        let inset = definition.optical_inset();
        let usable_w = (bounds.width - inset * 2.0).max(1.0);
        let usable_h = (bounds.height - inset * 2.0).max(1.0);
        let icon_scale = (usable_w.min(usable_h) / 24.0) * scale;
        let origin_x = (bounds.x + inset + (usable_w - icon_scale * 24.0 / scale) * 0.5) * scale;
        let origin_y = (bounds.y + inset + (usable_h - icon_scale * 24.0 / scale) * 0.5) * scale;
        let pixel_clip = physical_rect(
            clip.and_then(|clip| clip.intersection(bounds))
                .unwrap_or(bounds),
            scale,
            frame.width,
            frame.height,
        );
        let point = |source: [f32; 2]| {
            (
                origin_x + source[0] * icon_scale,
                origin_y + source[1] * icon_scale,
            )
        };
        let thickness = (definition.stroke_width() * icon_scale).max(1.0);
        for primitive in definition.primitives() {
            match primitive {
                IconPrimitive::Segment { from, to } => {
                    draw_line(
                        frame,
                        point(*from),
                        point(*to),
                        thickness,
                        color,
                        pixel_clip,
                    );
                }
                IconPrimitive::Polyline { points } => {
                    for segment in points.windows(2) {
                        draw_line(
                            frame,
                            point(segment[0]),
                            point(segment[1]),
                            thickness,
                            color,
                            pixel_clip,
                        );
                    }
                }
                IconPrimitive::Polygon { points } => {
                    let points = points.iter().copied().map(point).collect::<Vec<_>>();
                    fill_polygon(frame, &points, color, pixel_clip);
                }
                IconPrimitive::Circle { center, radius } => {
                    draw_circle(
                        frame,
                        point(*center),
                        radius * icon_scale,
                        thickness,
                        color,
                        pixel_clip,
                    );
                }
            }
        }
        Ok(())
    }
}

impl Default for CpuPainter {
    fn default() -> Self {
        Self::new()
    }
}

fn fill_rect(frame: &mut RasterFrame, bounds: Rect, color: Color, scale: f32, clip: Option<Rect>) {
    let Some(pixel_bounds) = physical_rect(bounds, scale, frame.width, frame.height) else {
        return;
    };
    let pixel_bounds =
        match clip.and_then(|clip| physical_rect(clip, scale, frame.width, frame.height)) {
            Some(clip) => intersect_i32(pixel_bounds, clip),
            None => Some(pixel_bounds),
        };
    let Some((left, top, right, bottom)) = pixel_bounds else {
        return;
    };
    for y in top..bottom {
        for x in left..right {
            blend(frame, x, y, color);
        }
    }
}

fn draw_stroke(
    frame: &mut RasterFrame,
    bounds: Rect,
    stroke_width: f32,
    color: Color,
    scale: f32,
    clip: Option<Rect>,
) {
    let stroke = stroke_width.max(1.0 / scale);
    for edge in [
        Rect {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: stroke,
        },
        Rect {
            x: bounds.x,
            y: bounds.y + bounds.height - stroke,
            width: bounds.width,
            height: stroke,
        },
        Rect {
            x: bounds.x,
            y: bounds.y,
            width: stroke,
            height: bounds.height,
        },
        Rect {
            x: bounds.x + bounds.width - stroke,
            y: bounds.y,
            width: stroke,
            height: bounds.height,
        },
    ] {
        fill_rect(frame, edge, color, scale, clip);
    }
}

fn draw_mask(
    frame: &mut RasterFrame,
    origin: (i32, i32),
    extent: (u32, u32),
    mask: &[u8],
    color: Color,
    clip: Option<(i32, i32, i32, i32)>,
) {
    let (left, top) = origin;
    let (width, height) = extent;
    let expected = usize::try_from(width)
        .ok()
        .and_then(|width| usize::try_from(height).ok().map(|height| width * height));
    if expected != Some(mask.len()) {
        return;
    }
    for row in 0..height {
        for column in 0..width {
            let x = left + column as i32;
            let y = top + row as i32;
            if !inside_clip(x, y, clip) {
                continue;
            }
            let index = row as usize * width as usize + column as usize;
            let alpha = u16::from(color.0[3]) * u16::from(mask[index]) / 255;
            blend(
                frame,
                x,
                y,
                Color([color.0[0], color.0[1], color.0[2], alpha as u8]),
            );
        }
    }
}

fn draw_line(
    frame: &mut RasterFrame,
    from: (f32, f32),
    to: (f32, f32),
    thickness: f32,
    color: Color,
    clip: Option<(i32, i32, i32, i32)>,
) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as u32;
    let radius = (thickness * 0.5).ceil() as i32;
    for step in 0..=steps {
        let amount = step as f32 / steps as f32;
        let x = (from.0 + dx * amount).round() as i32;
        let y = (from.1 + dy * amount).round() as i32;
        for offset_y in -radius..=radius {
            for offset_x in -radius..=radius {
                if (offset_x * offset_x + offset_y * offset_y) as f32
                    <= (thickness * 0.5 + 0.5).powi(2)
                    && inside_clip(x + offset_x, y + offset_y, clip)
                {
                    blend(frame, x + offset_x, y + offset_y, color);
                }
            }
        }
    }
}

fn fill_polygon(
    frame: &mut RasterFrame,
    points: &[(f32, f32)],
    color: Color,
    clip: Option<(i32, i32, i32, i32)>,
) {
    if points.len() < 3 {
        return;
    }
    let min_x = points
        .iter()
        .map(|point| point.0)
        .fold(f32::INFINITY, f32::min)
        .floor() as i32;
    let max_x = points
        .iter()
        .map(|point| point.0)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as i32;
    let min_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::INFINITY, f32::min)
        .floor() as i32;
    let max_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if inside_clip(x, y, clip) && point_in_polygon(x as f32 + 0.5, y as f32 + 0.5, points) {
                blend(frame, x, y, color);
            }
        }
    }
}

fn point_in_polygon(x: f32, y: f32, points: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut previous = points.len() - 1;
    for current in 0..points.len() {
        let (xi, yi) = points[current];
        let (xj, yj) = points[previous];
        let crosses = (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi;
        if crosses {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn draw_circle(
    frame: &mut RasterFrame,
    center: (f32, f32),
    radius: f32,
    thickness: f32,
    color: Color,
    clip: Option<(i32, i32, i32, i32)>,
) {
    let outer = radius + thickness * 0.5;
    let inner = (radius - thickness * 0.5).max(0.0);
    let left = (center.0 - outer).floor() as i32;
    let right = (center.0 + outer).ceil() as i32;
    let top = (center.1 - outer).floor() as i32;
    let bottom = (center.1 + outer).ceil() as i32;
    for y in top..=bottom {
        for x in left..=right {
            let dx = x as f32 + 0.5 - center.0;
            let dy = y as f32 + 0.5 - center.1;
            let distance = (dx * dx + dy * dy).sqrt();
            if (inner..=outer).contains(&distance) && inside_clip(x, y, clip) {
                blend(frame, x, y, color);
            }
        }
    }
}

fn blend(frame: &mut RasterFrame, x: i32, y: i32, source: Color) {
    if x < 0 || y < 0 || x >= frame.width as i32 || y >= frame.height as i32 {
        return;
    }
    let index = (y as usize * frame.width as usize + x as usize) * 4;
    let alpha = u16::from(source.0[3]);
    let inverse = 255 - alpha;
    for channel in 0..3 {
        frame.pixels[index + channel] = ((u16::from(source.0[channel]) * alpha
            + u16::from(frame.pixels[index + channel]) * inverse
            + 127)
            / 255) as u8;
    }
    frame.pixels[index + 3] = 255;
}

fn physical_rect(
    bounds: Rect,
    scale: f32,
    frame_width: u32,
    frame_height: u32,
) -> Option<(i32, i32, i32, i32)> {
    let left = (bounds.x * scale).floor().max(0.0) as i32;
    let top = (bounds.y * scale).floor().max(0.0) as i32;
    let right = ((bounds.x + bounds.width) * scale)
        .ceil()
        .min(frame_width as f32) as i32;
    let bottom = ((bounds.y + bounds.height) * scale)
        .ceil()
        .min(frame_height as f32) as i32;
    (right > left && bottom > top).then_some((left, top, right, bottom))
}

fn intersect_i32(
    left: (i32, i32, i32, i32),
    right: (i32, i32, i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let x0 = left.0.max(right.0);
    let y0 = left.1.max(right.1);
    let x1 = left.2.min(right.2);
    let y1 = left.3.min(right.3);
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

fn inside_clip(x: i32, y: i32, clip: Option<(i32, i32, i32, i32)>) -> bool {
    clip.map_or(true, |(left, top, right, bottom)| {
        x >= left && y >= top && x < right && y < bottom
    })
}
