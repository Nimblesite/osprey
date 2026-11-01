//! A scene's sprite images: the atlas paths a scene names, decoded once, and each sprite's
//! turned copy, rotated on the CPU because GPUI draws images unrotated ([UI-SCENE],
//! [UI-MACHINE-CANVAS]).

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::*;
use image::{Frame, RgbaImage};

/// How finely a sprite's angle is quantized: 240 steps a turn, 1.5 degrees each.
const STEPS: f32 = 240.;

/// Turned copies kept at once; older ones are released from GPUI's atlas.
const KEPT: usize = 8;

/// Decoded atlas images by path and the most recent turned copies.
#[derive(Default)]
pub struct Sprites {
    sources: HashMap<String, Result<Arc<RgbaImage>, String>>,
    turned: Vec<((String, i32), Arc<RenderImage>)>,
    released: Vec<Arc<RenderImage>>,
}

impl Sprites {
    /// The image for `path` turned by `angle` radians, decoding and turning only once per step.
    pub fn image(&mut self, path: &str, angle: f32) -> Result<Arc<RenderImage>, String> {
        let step = (angle / std::f32::consts::TAU * STEPS).round() as i32 % STEPS as i32;
        let key = (path.to_owned(), step);
        if let Some((_, found)) = self.turned.iter().find(|(held, _)| *held == key) {
            return Ok(found.clone());
        }
        let source = self
            .sources
            .entry(path.to_owned())
            .or_insert_with(|| load(path))
            .clone()?;
        let made = Arc::new(render(turn(&source, step as f32 / STEPS * std::f32::consts::TAU)));
        self.turned.push((key, made.clone()));
        if self.turned.len() > KEPT {
            let (_, old) = self.turned.remove(0);
            self.released.push(old);
        }
        Ok(made)
    }

    /// Turned copies no longer drawn, for the window to drop from GPUI's atlas.
    pub fn released(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.released)
    }
}

/// Decodes a PNG or JPEG from disk.
fn load(path: &str) -> Result<Arc<RgbaImage>, String> {
    image::open(path)
        .map(|decoded| Arc::new(decoded.to_rgba8()))
        .map_err(|problem| format!("sprite {path} could not be loaded: {problem}"))
}

/// Turns an image about its centre by `angle` radians with bilinear sampling, keeping its size.
pub fn turn(source: &RgbaImage, angle: f32) -> RgbaImage {
    if angle == 0. {
        return source.clone();
    }
    let (w, h) = source.dimensions();
    let (cx, cy) = (w as f32 / 2., h as f32 / 2.);
    let (sin, cos) = angle.sin_cos();
    RgbaImage::from_fn(w, h, |x, y| {
        let dx = x as f32 + 0.5 - cx;
        let dy = y as f32 + 0.5 - cy;
        sample(source, cos * dx + sin * dy + cx - 0.5, -sin * dx + cos * dy + cy - 0.5)
    })
}

/// A bilinear sample; outside the image is transparent.
fn sample(source: &RgbaImage, x: f32, y: f32) -> image::Rgba<u8> {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |dx: f32, dy: f32| -> [f32; 4] {
        let (px, py) = (x0 + dx, y0 + dy);
        if px < 0. || py < 0. || px >= source.width() as f32 || py >= source.height() as f32 {
            return [0.; 4];
        }
        source.get_pixel(px as u32, py as u32).0.map(f32::from)
    };
    let (a, b, c, d) = (at(0., 0.), at(1., 0.), at(0., 1.), at(1., 1.));
    let mut out = [0u8; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bottom = c[i] + (d[i] - c[i]) * fx;
        out[i] = (top + (bottom - top) * fy).round().clamp(0., 255.) as u8;
    }
    image::Rgba(out)
}

/// GPUI's image form: BGRA bytes in one frame.
fn render(mut pixels: RgbaImage) -> RenderImage {
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    RenderImage::new(vec![Frame::new(pixels)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A half turn moves the top-left pixel to the bottom-right and keeps the size.
    #[test]
    fn a_half_turn_swaps_opposite_corners() {
        let mut source = RgbaImage::new(4, 4);
        source.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let turned = turn(&source, std::f32::consts::PI);
        assert_eq!(turned.dimensions(), (4, 4));
        assert_eq!(turned.get_pixel(3, 3).0, [255, 0, 0, 255]);
        assert_eq!(turned.get_pixel(0, 0).0[3], 0);
    }

    /// A missing file is a named error, kept so it is not retried every frame.
    #[test]
    fn a_missing_sprite_is_an_error_naming_its_path() {
        let mut sprites = Sprites::default();
        let problem = sprites.image("/no/such/sprite.png", 0.).err().unwrap_or_default();
        assert!(problem.contains("/no/such/sprite.png"));
        assert!(sprites.sources.contains_key("/no/such/sprite.png"));
    }
}
