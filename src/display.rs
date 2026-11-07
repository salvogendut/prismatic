use sdl3::rect::Rect;

// The SDL side accepts packed RGBA bytes, with no dependency on Iris internals.
// Iris publishes frames in this format from its refresh thread.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    /// Decode the embedded artwork once, separately from the guest framebuffer.
    pub fn logo() -> Result<Self, Box<dyn std::error::Error>> {
        decode_logo(include_bytes!("../assets/prismatic_logo.png"))
    }

    pub fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: [0, 0, 0, 255].repeat(width as usize * height as usize),
        }
    }

    /// Guest heads share a window and captures. Until one head publishes, reserve
    /// a black area matching the other head; preserve each head's native pixels.
    pub fn side_by_side(left: Option<&Self>, right: Option<&Self>) -> Self {
        let fallback = left.or(right).map_or((640, 480), |f| (f.width, f.height));
        let size = |frame: Option<&Self>| frame.map_or(fallback, |f| (f.width, f.height));
        let (lw, lh) = size(left);
        let (rw, rh) = size(right);
        let mut result = Self::blank(lw + rw, lh.max(rh));
        for (frame, x) in [(left, 0), (right, lw)] {
            if let Some(frame) = frame {
                for y in 0..frame.height as usize {
                    let source = y * frame.width as usize * 4;
                    let dest = (y * result.width as usize + x as usize) * 4;
                    result.rgba[dest..dest + frame.width as usize * 4]
                        .copy_from_slice(&frame.rgba[source..source + frame.width as usize * 4]);
                }
            }
        }
        result
    }

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

fn decode_logo(bytes: &[u8]) -> Result<Frame, Box<dyn std::error::Error>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("Logo is too large")?];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels,
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return Err("Logo must be RGB or RGBA".into()),
    };
    Ok(Frame {
        width: info.width,
        height: info.height,
        rgba,
    })
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

/// SDL events use window points; the renderer letterboxes a fixed logical view.
/// Converting through output pixels also handles high-DPI window/output sizes.
/// Use the window's pixel size for `output`: SDL's current render output size
/// already includes logical presentation and would lose the letterbox offsets.
pub fn window_to_view(
    point: (f32, f32),
    window: (u32, u32),
    output: (u32, u32),
    view: (u32, u32),
) -> Option<(f32, f32)> {
    if [window.0, window.1, output.0, output.1, view.0, view.1].contains(&0) {
        return None;
    }
    let scale = (output.0 as f32 / view.0 as f32).min(output.1 as f32 / view.1 as f32);
    Some((
        (point.0 * output.0 as f32 / window.0 as f32
            - (output.0 as f32 - view.0 as f32 * scale) / 2.0)
            / scale,
        (point.1 * output.1 as f32 / window.1 as f32
            - (output.1 as f32 - view.1 as f32 * scale) / 2.0)
            / scale,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_logo_decodes_with_transparency_and_invalid_png_is_rejected() {
        let logo = Frame::logo().unwrap();
        assert!(logo.width > 0 && logo.height > 0);
        assert_eq!(logo.rgba.len(), (logo.width * logo.height * 4) as usize);
        assert!(logo.rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 255));
        assert!(logo.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
        assert!(decode_logo(b"invalid PNG").is_err());
    }

    #[test]
    fn pointer_conversion_accounts_for_hidpi_and_letterboxing() {
        assert_eq!(
            window_to_view((600.0, 360.0), (1200, 720), (2400, 1440), (960, 720)),
            Some((480.0, 360.0))
        );
        assert_eq!(
            window_to_view((100.0, 360.0), (1200, 720), (2400, 1440), (960, 720)),
            Some((-20.0, 360.0))
        );
        assert_eq!(window_to_view((0.0, 0.0), (0, 0), (0, 0), (960, 720)), None);
    }

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
