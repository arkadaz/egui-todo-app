//! The background image, made the right size once, when it's chosen.
//!
//! Flutter decodes the background whenever the Focus tab shows it, and a GIF again for
//! every frame it plays. A 12-megapixel photo, or a GIF as big as a desktop window, costs
//! memory and time on every one of those decodes for no visible gain on a phone. So here
//! it's shrunk to what the screen can show: stills to the screen's longest side,
//! animations (every frame) to at most [`MAX_ANIMATION_SIDE`]. Images that already fit
//! are kept byte for byte.
//!
//! The slow part of shrinking a GIF is choosing each frame's 256 colors, so frames are
//! processed on all CPU cores at once.

use anyhow::{bail, Context, Result};
use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::{self, FilterType};
use image::metadata::Orientation;
use image::{AnimationDecoder, DynamicImage, Frame, ImageDecoder, ImageFormat, ImageReader, RgbaImage};
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::thread;

/// Animation frames are decoded over and over while they play, so they're kept smaller than
/// a still image (and a GIF's 256 colors gain little from more pixels).
pub const MAX_ANIMATION_SIDE: u32 = 1280;
/// Screens report sizes in this range; anything outside is treated as the nearest end.
const SCREEN_SIDE_RANGE: (u32, u32) = (480, 4096);
/// 1 (best colors, slowest) to 30. 10 is the gif crate's own default.
const GIF_QUANTIZE_SPEED: i32 = 10;
const JPEG_QUALITY: u8 = 90;

const NOT_AN_IMAGE: &str = "Please choose a GIF, PNG, JPG or WebP image.";
const UNREADABLE: &str = "That image couldn't be read. It may be damaged.";

/// An image ready to be saved as the background.
#[derive(Debug)]
pub struct Prepared {
    pub bytes: Vec<u8>,
    /// "gif", "png", "jpg" or "webp"
    pub extension: &'static str,
    /// Width and height of the chosen file (after turning it upright).
    pub original_size: (u32, u32),
    /// Width and height after shrinking (the same as `original_size` if it already fit).
    pub size: (u32, u32),
    /// 1 for a still image.
    pub frames: u32,
}

impl Prepared {
    pub fn was_shrunk(&self) -> bool {
        self.size != self.original_size
    }
}

/// Makes `bytes` (a GIF, PNG, JPG or WebP file) ready to be the background on a screen
/// whose longest side is `screen_side` pixels.
pub fn prepare_background(bytes: &[u8], screen_side: u32) -> Result<Prepared> {
    let screen_side = screen_side.clamp(SCREEN_SIDE_RANGE.0, SCREEN_SIDE_RANGE.1);
    let animation_side = screen_side.min(MAX_ANIMATION_SIDE);
    match image::guess_format(bytes).ok() {
        Some(ImageFormat::Gif) => {
            let decoder = GifDecoder::new(Cursor::new(bytes)).context(UNREADABLE)?;
            prepare_animation(decoder, Some(bytes), animation_side)
        }
        Some(ImageFormat::WebP) => {
            let decoder = WebPDecoder::new(Cursor::new(bytes)).context(UNREADABLE)?;
            if decoder.has_animation() {
                // Animated WebP becomes a GIF: Flutter plays both, and GIF encoding is built in.
                prepare_animation(decoder, None, animation_side)
            } else {
                prepare_still(bytes, ImageFormat::WebP, screen_side)
            }
        }
        Some(format @ (ImageFormat::Png | ImageFormat::Jpeg)) => prepare_still(bytes, format, screen_side),
        _ => bail!(NOT_AN_IMAGE),
    }
}

/// The largest size with the same shape as `width` x `height` that fits in a
/// `max_side` x `max_side` square. Never enlarges.
fn fit(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= max_side {
        return (width, height);
    }
    let scale = |side: u32| {
        ((u64::from(side) * u64::from(max_side) + u64::from(longest) / 2) / u64::from(longest)).max(1) as u32
    };
    (scale(width), scale(height))
}

fn extension_of(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Gif => "gif",
        ImageFormat::Png => "png",
        ImageFormat::WebP => "webp",
        _ => "jpg",
    }
}

fn prepare_still(bytes: &[u8], format: ImageFormat, max_side: u32) -> Result<Prepared> {
    let mut decoder = ImageReader::with_format(Cursor::new(bytes), format)
        .into_decoder()
        .context(UNREADABLE)?;
    // Phone photos are often stored sideways, with a note saying which way is up.
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (width, height) = decoder.dimensions();
    let size = fit(width, height, max_side);
    let turned = |(w, h): (u32, u32)| match orientation {
        Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH => {
            (h, w)
        }
        _ => (w, h),
    };

    if size == (width, height) && orientation == Orientation::NoTransforms {
        return Ok(Prepared {
            bytes: bytes.to_vec(),
            extension: extension_of(format),
            original_size: (width, height),
            size,
            frames: 1,
        });
    }

    let mut image = DynamicImage::from_decoder(decoder).context(UNREADABLE)?;
    if size != (width, height) {
        image = image.resize_exact(size.0, size.1, FilterType::Lanczos3);
    }
    image.apply_orientation(orientation);

    let mut out = Vec::new();
    let extension = if image.color().has_alpha() {
        image.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;
        "png"
    } else {
        JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&image.to_rgb8())?;
        "jpg"
    };
    Ok(Prepared {
        bytes: out,
        extension,
        original_size: turned((width, height)),
        size: turned(size),
        frames: 1,
    })
}

/// Shrinks every frame of an animation and writes it as a GIF. `original` is the file
/// itself if it's a GIF, so it can be kept as it is when it already fits.
fn prepare_animation<'a, D>(decoder: D, original: Option<&[u8]>, max_side: u32) -> Result<Prepared>
where
    D: ImageDecoder + AnimationDecoder<'a>,
{
    let (width, height) = decoder.dimensions();
    let size = fit(width, height, max_side);
    let frames = decoder.into_frames();

    if let (Some(original), true) = (original, size == (width, height)) {
        return Ok(Prepared {
            bytes: original.to_vec(),
            extension: "gif",
            original_size: (width, height),
            size,
            frames: frames.count() as u32,
        });
    }

    let gif_size = (
        u16::try_from(size.0).context("That animation is too wide.")?,
        u16::try_from(size.1).context("That animation is too tall.")?,
    );
    let mut out = Vec::new();
    let mut count = 0;
    {
        let mut encoder = gif::Encoder::new(&mut out, gif_size.0, gif_size.1, &[])?;
        encoder.set_repeat(gif::Repeat::Infinite)?;
        // Decoding is sequential, but each frame's shrinking and color choice is independent:
        // take one frame per core, process them together, write them in order, repeat.
        let batch_size = thread::available_parallelism().map_or(4, NonZeroUsize::get).min(8);
        let mut frames = frames.peekable();
        while frames.peek().is_some() {
            let batch = frames
                .by_ref()
                .take(batch_size)
                .collect::<Result<Vec<_>, _>>()
                .context(UNREADABLE)?;
            count += batch.len() as u32;
            for frame in shrink_frames(batch, size, gif_size) {
                encoder.write_frame(&frame)?;
            }
        }
    } // dropping the encoder writes the end of the file
    if count == 0 {
        bail!(UNREADABLE);
    }
    Ok(Prepared {
        bytes: out,
        extension: "gif",
        original_size: (width, height),
        size,
        frames: count,
    })
}

/// Resizes and color-reduces a batch of frames, one thread per frame. Keeps their order.
fn shrink_frames(batch: Vec<Frame>, size: (u32, u32), gif_size: (u16, u16)) -> Vec<gif::Frame<'static>> {
    thread::scope(|scope| {
        let workers: Vec<_> = batch
            .into_iter()
            .map(|frame| {
                scope.spawn(move || {
                    // Frame delays are milliseconds here and hundredths of a second in a GIF.
                    let (numer, denom) = frame.delay().numer_denom_ms();
                    let delay_ms = numer / denom.max(1);
                    let full: RgbaImage = frame.into_buffer();
                    let mut pixels = if full.dimensions() == size {
                        full
                    } else {
                        imageops::resize(&full, size.0, size.1, FilterType::Triangle)
                    };
                    let mut gif_frame =
                        gif::Frame::from_rgba_speed(gif_size.0, gif_size.1, &mut pixels, GIF_QUANTIZE_SPEED);
                    gif_frame.delay = u16::try_from(delay_ms / 10).unwrap_or(u16::MAX);
                    // Each frame is a whole picture, so clear before drawing the next one.
                    gif_frame.dispose = gif::DisposalMethod::Background;
                    gif_frame
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("a frame worker panicked"))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, ImageEncoder, Rgba};

    fn gif_bytes(width: u32, height: u32, frames: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = GifEncoder::new_with_speed(&mut out, 30);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for i in 0..frames {
                let shade = (i * 60 % 256) as u8;
                let buffer = RgbaImage::from_pixel(width, height, Rgba([shade, 100, 200, 255]));
                encoder
                    .encode_frame(Frame::from_parts(buffer, 0, 0, Delay::from_numer_denom_ms(80, 1)))
                    .unwrap();
            }
        }
        out
    }

    fn encoded(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        image.write_to(&mut Cursor::new(&mut out), format).unwrap();
        out
    }

    #[test]
    fn fit_keeps_the_shape_and_never_enlarges() {
        assert_eq!(fit(4000, 3000, 2000), (2000, 1500));
        assert_eq!(fit(3000, 4000, 2000), (1500, 2000));
        assert_eq!(fit(800, 600, 2000), (800, 600));
        assert_eq!(fit(10_000, 1, 1000), (1000, 1), "never shrinks a side to zero");
    }

    #[test]
    fn a_big_photo_is_shrunk_to_the_screen() {
        let photo = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(3000, 2000, image::Rgb([200, 120, 40])));
        let prepared = prepare_background(&encoded(&photo, ImageFormat::Jpeg), 1500).unwrap();
        assert_eq!(prepared.extension, "jpg");
        assert_eq!((prepared.original_size, prepared.size), ((3000, 2000), (1500, 1000)));
        assert!(prepared.was_shrunk());
        let back = image::load_from_memory(&prepared.bytes).unwrap();
        assert_eq!((back.width(), back.height()), (1500, 1000));
    }

    #[test]
    fn transparency_is_kept_as_png() {
        let sticker = DynamicImage::ImageRgba8(RgbaImage::from_pixel(3000, 3000, Rgba([0, 0, 0, 0])));
        let prepared = prepare_background(&encoded(&sticker, ImageFormat::Png), 1000).unwrap();
        assert_eq!((prepared.extension, prepared.size), ("png", (1000, 1000)));
        assert!(image::load_from_memory(&prepared.bytes).unwrap().color().has_alpha());
    }

    #[test]
    fn an_image_that_fits_is_kept_byte_for_byte() {
        let small = encoded(
            &DynamicImage::ImageRgb8(image::RgbImage::new(640, 480)),
            ImageFormat::Png,
        );
        let prepared = prepare_background(&small, 2000).unwrap();
        assert_eq!(prepared.bytes, small);
        assert!(!prepared.was_shrunk());

        let gif = gif_bytes(300, 200, 3);
        let prepared = prepare_background(&gif, 2000).unwrap();
        assert_eq!((prepared.bytes == gif, prepared.frames), (true, 3));
    }

    #[test]
    fn a_big_gif_keeps_every_frame_and_its_timing() {
        let prepared = prepare_background(&gif_bytes(2560, 1600, 11), 3000).unwrap();
        assert_eq!(
            prepared.size,
            (MAX_ANIMATION_SIDE, 800),
            "animations are capped below the screen size"
        );
        assert_eq!(prepared.frames, 11);

        let decoder = GifDecoder::new(Cursor::new(&prepared.bytes)).unwrap();
        assert_eq!(decoder.dimensions(), (1280, 800));
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 11, "frames come back in full, in order");
        assert_eq!(frames[0].delay().numer_denom_ms(), (80, 1));
        let shade = |i: usize| i32::from(frames[i].buffer().get_pixel(640, 400)[0]);
        assert!(
            (shade(1) - 60).abs() <= 8 && (shade(4) - 240).abs() <= 8,
            "frame order is kept"
        );
    }

    #[test]
    fn a_sideways_photo_is_turned_upright() {
        // Stored 400 wide and 200 tall, with EXIF saying "turn 90° clockwise to view".
        let exif_rotate_90 = vec![
            b'I', b'I', 42, 0, 8, 0, 0, 0, // little-endian TIFF, first directory at byte 8
            1, 0, // one entry:
            0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, // Orientation (0x0112), 1 SHORT, value 6
            0, 0, 0, 0, // no next directory
        ];
        let mut photo = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut photo, 90);
        encoder.set_exif_metadata(exif_rotate_90).unwrap();
        encoder
            .write_image(
                &image::RgbImage::new(400, 200),
                400,
                200,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();

        let prepared = prepare_background(&photo, 1000).unwrap();
        assert_eq!((prepared.original_size, prepared.size), ((200, 400), (200, 400)));
        assert!(!prepared.was_shrunk(), "turning isn't shrinking");
        let back = image::load_from_memory(&prepared.bytes).unwrap();
        assert_eq!((back.width(), back.height()), (200, 400), "saved upright");
    }

    #[test]
    fn other_files_are_refused_with_a_clear_message() {
        let error = prepare_background(b"just some text", 1000).unwrap_err();
        assert_eq!(error.to_string(), NOT_AN_IMAGE);
        let error = prepare_background(b"GIF89a broken", 1000).unwrap_err();
        assert_eq!(error.to_string(), UNREADABLE);
    }
}
