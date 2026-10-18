//! The project thumbnail: a screenshot of the viewport, cut to 16:9, shrunk
//! and saved as a PNG in the project folder for the start screen.

use eframe::egui;

/// The thumbnail's size, in pixels.
const WIDTH: usize = 320;
const HEIGHT: usize = 180;

/// Cuts the part of `screenshot` under `rect` (in points) to the largest
/// centered 16:9 area, shrinks it, and returns it as PNG bytes. `None` if
/// the area is empty or off the screenshot.
pub fn encode(
    screenshot: &egui::ColorImage,
    rect: egui::Rect,
    pixels_per_point: f32,
) -> Option<Vec<u8>> {
    let [full_width, full_height] = screenshot.size;
    let left = ((rect.min.x * pixels_per_point).round().max(0.0) as usize).min(full_width);
    let top = ((rect.min.y * pixels_per_point).round().max(0.0) as usize).min(full_height);
    let right = ((rect.max.x * pixels_per_point).round().max(0.0) as usize).min(full_width);
    let bottom = ((rect.max.y * pixels_per_point).round().max(0.0) as usize).min(full_height);
    let (area_width, area_height) = (right.checked_sub(left)?, bottom.checked_sub(top)?);
    if area_width == 0 || area_height == 0 {
        return None;
    }

    // The largest 16:9 area that fits, in the middle.
    let (crop_width, crop_height) = if area_width * HEIGHT > area_height * WIDTH {
        (area_height * WIDTH / HEIGHT, area_height)
    } else {
        (area_width, area_width * HEIGHT / WIDTH)
    };
    if crop_width == 0 || crop_height == 0 {
        return None;
    }
    let crop_left = left + (area_width - crop_width) / 2;
    let crop_top = top + (area_height - crop_height) / 2;

    // Each thumbnail pixel is the average of the block of screenshot pixels it covers.
    let mut rgb = Vec::with_capacity(WIDTH * HEIGHT * 3);
    for y in 0..HEIGHT {
        let y0 = crop_top + y * crop_height / HEIGHT;
        let y1 = (crop_top + (y + 1) * crop_height / HEIGHT).max(y0 + 1);
        for x in 0..WIDTH {
            let x0 = crop_left + x * crop_width / WIDTH;
            let x1 = (crop_left + (x + 1) * crop_width / WIDTH).max(x0 + 1);
            let mut sum = [0u32; 3];
            let mut count = 0;
            for sy in y0..y1.min(full_height) {
                for sx in x0..x1.min(full_width) {
                    let [r, g, b, _] = screenshot.pixels[sy * full_width + sx].to_array();
                    sum[0] += u32::from(r);
                    sum[1] += u32::from(g);
                    sum[2] += u32::from(b);
                    count += 1;
                }
            }
            let count = count.max(1);
            rgb.extend(sum.map(|channel| (channel / count) as u8));
        }
    }

    let mut png_bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut png_bytes, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().ok()?.write_image_data(&rgb).ok()?;
    Some(png_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_viewport_is_cropped_to_a_16_by_9_png() {
        let screenshot = egui::ColorImage::filled([800, 600], egui::Color32::from_rgb(10, 20, 30));
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(400.0, 400.0));
        let bytes = encode(&screenshot, rect, 1.0).unwrap();
        let decoded = eframe::icon_data::from_png_bytes(&bytes).unwrap();
        assert_eq!(
            (decoded.width, decoded.height),
            (WIDTH as u32, HEIGHT as u32)
        );
        assert_eq!(&decoded.rgba[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn an_area_off_the_screenshot_makes_no_thumbnail() {
        let screenshot = egui::ColorImage::filled([100, 100], egui::Color32::BLACK);
        let rect = egui::Rect::from_min_size(egui::pos2(500.0, 500.0), egui::vec2(50.0, 50.0));
        assert!(encode(&screenshot, rect, 1.0).is_none());
    }
}
