//! Tray ring icon, rendered in code at runtime.
//!
//! The ring shows how full the system drive is: the indigo arc sweeps
//! clockwise from the top for the used fraction of the disk, over a faint
//! neutral track. The image is rasterised with simple distance/angle math
//! and 4x supersampling for anti-aliasing — there are no image assets.

/// Accent indigo used for the "used" arc.
const ACCENT: (u8, u8, u8) = (0x58, 0x56, 0xD6);
/// Neutral track colour (the not-yet-used remainder of the ring).
const TRACK: (u8, u8, u8) = (0x8E, 0x8E, 0x93);
/// Track alpha: kept translucent so the ring reads as a background gauge.
const TRACK_ALPHA: u8 = 110;

/// Render an RGBA ring image of `size` x `size` pixels.
///
/// `used_fraction` is clamped to `0.0..=1.0`; `0.0` draws an empty track,
/// `1.0` a complete indigo ring.
pub fn render_rgba(used_fraction: f64, size: u32) -> Vec<u8> {
    let fraction = used_fraction.clamp(0.0, 1.0);
    let ss: u32 = 4; // supersampling factor per axis
    let n = size * ss;
    let mut buf = vec![0u8; (size * size * 4) as usize];

    let center = (n as f64 - 1.0) / 2.0;
    let outer = n as f64 * 0.46;
    let thickness = (n as f64 * 0.15).max(1.0);
    let inner = outer - thickness;
    let sweep = fraction * std::f64::consts::TAU;

    for y in 0..size {
        for x in 0..size {
            // Accumulate supersampled sub-pixels (premultiplied-style average
            // over straight RGBA is fine at these sizes).
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let px = (x * ss + sx) as f64;
                    let py = (y * ss + sy) as f64;
                    let dx = px - center;
                    let dy = py - center;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist < inner || dist > outer {
                        continue;
                    }
                    // Angle clockwise from 12 o'clock, in [0, TAU).
                    let mut angle = dx.atan2(-dy);
                    if angle < 0.0 {
                        angle += std::f64::consts::TAU;
                    }
                    if angle <= sweep {
                        r += ACCENT.0 as u32 * 255;
                        g += ACCENT.1 as u32 * 255;
                        b += ACCENT.2 as u32 * 255;
                        a += 255;
                    } else {
                        r += TRACK.0 as u32 * TRACK_ALPHA as u32;
                        g += TRACK.1 as u32 * TRACK_ALPHA as u32;
                        b += TRACK.2 as u32 * TRACK_ALPHA as u32;
                        a += TRACK_ALPHA as u32;
                    }
                }
            }
            let samples = ss * ss;
            let idx = ((y * size + x) * 4) as usize;
            if a > 0 {
                // Un-premultiply the accumulated colour by the coverage alpha.
                let coverage = a / samples; // 0..=255 average alpha
                buf[idx] = (r / samples / 255).min(255) as u8;
                buf[idx + 1] = (g / samples / 255).min(255) as u8;
                buf[idx + 2] = (b / samples / 255).min(255) as u8;
                buf[idx + 3] = coverage as u8;
            }
        }
    }
    buf
}

/// Render the ring as a Tauri image of `size` x `size` pixels.
pub fn image(used_fraction: f64, size: u32) -> tauri::image::Image<'static> {
    tauri::image::Image::new_owned(render_rgba(used_fraction, size), size, size)
}
