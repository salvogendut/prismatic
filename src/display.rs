use sdl3::rect::Rect;

// The SDL side accepts packed RGBA bytes, with no dependency on Iris internals.
// A future Iris renderer will publish frames in this format on its refresh thread.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    pub fn preview() -> Self {
        let (width, height) = (640, 480);
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let grid = u8::from(x % 40 == 0 || y % 40 == 0) * 12;
                rgba.extend_from_slice(&[
                    13 + (x * 12 / width) as u8 + grid,
                    18 + (y * 12 / height) as u8 + grid,
                    30 + (x * 18 / width) as u8 + grid,
                    255,
                ]);
            }
        }
        Self {
            width: width as u32,
            height: height as u32,
            rgba,
        }
    }
}

pub fn fit(width: u32, height: u32, area: Rect) -> Rect {
    let scale = (area.width() as f64 / width as f64).min(area.height() as f64 / height as f64);
    let w = (width as f64 * scale).round() as u32;
    let h = (height as f64 * scale).round() as u32;
    Rect::new(
        area.x() + ((area.width() - w) / 2) as i32,
        area.y() + ((area.height() - h) / 2) as i32,
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_aspect_ratio_is_preserved_inside_status_strip_area() {
        assert_eq!(
            fit(640, 480, Rect::new(0, 0, 960, 680)),
            Rect::new(26, 0, 907, 680)
        );
        assert_eq!(
            fit(1280, 1024, Rect::new(0, 0, 800, 800)),
            Rect::new(0, 80, 800, 640)
        );
    }
}
