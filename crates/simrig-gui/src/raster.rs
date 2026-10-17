//! Rotated layers drawn on the CPU, for renderers that cannot rotate images (Slint's software
//! renderer, used without OpenGL and for screenshots). With the GPU renderers the interface
//! rotates the layers itself and this is not used.

use std::collections::HashMap;

use resvg::{tiny_skia, usvg};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const WHEEL_ROUND: &[u8] = include_bytes!("../../../assets/devices/wheel-round.svg");
const WHEEL_GT: &[u8] = include_bytes!("../../../assets/devices/wheel-gt.svg");
const WHEEL_FORMULA: &[u8] = include_bytes!("../../../assets/devices/wheel-formula.svg");
const LEVER: &[u8] = include_bytes!("../../../assets/devices/handbrake-lever.svg");

/// Angles are rounded to this step, so a still device draws nothing new.
const STEP_DEG: f32 = 0.5;
const MAX_CACHED: usize = 64;

#[derive(Default)]
pub struct Raster {
    trees: HashMap<&'static str, usvg::Tree>,
    frames: HashMap<(&'static str, i32), Image>,
}

impl Raster {
    pub fn wheel(&mut self, rim: &str, angle: f32, pivot: (f32, f32)) -> Image {
        let layer = match rim {
            "gt" => "wheel-gt",
            "formula" => "wheel-formula",
            _ => "wheel-round",
        };
        self.frame(layer, angle, pivot)
    }

    pub fn lever(&mut self, angle: f32, pivot: (f32, f32)) -> Image {
        self.frame("handbrake-lever", angle, pivot)
    }

    fn frame(&mut self, layer: &'static str, angle: f32, pivot: (f32, f32)) -> Image {
        let step = (angle / STEP_DEG).round() as i32;
        if let Some(image) = self.frames.get(&(layer, step)) {
            return image.clone();
        }
        if self.frames.len() >= MAX_CACHED {
            self.frames.clear();
        }
        let image = self.render(layer, step as f32 * STEP_DEG, pivot);
        self.frames.insert((layer, step), image.clone());
        image
    }

    fn render(&mut self, layer: &'static str, angle: f32, pivot: (f32, f32)) -> Image {
        let tree = self.trees.entry(layer).or_insert_with(|| {
            let data = match layer {
                "wheel-gt" => WHEEL_GT,
                "wheel-formula" => WHEEL_FORMULA,
                "handbrake-lever" => LEVER,
                _ => WHEEL_ROUND,
            };
            usvg::Tree::from_data(data, &usvg::Options::default()).expect("core drawings parse")
        });
        let size = tree.size().to_int_size();
        let Some(mut pixmap) = tiny_skia::Pixmap::new(size.width(), size.height()) else {
            return Image::default();
        };
        let transform = tiny_skia::Transform::from_rotate_at(angle, pivot.0, pivot.1);
        resvg::render(tree, transform, &mut pixmap.as_mut());
        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            pixmap.data(),
            size.width(),
            size.height(),
        );
        Image::from_rgba8_premultiplied(buffer)
    }
}
