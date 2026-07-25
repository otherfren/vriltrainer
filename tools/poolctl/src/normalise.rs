//! The anti-leakage pipeline (D5).
//!
//! Every image leaves here with the same dimensions, the same colour depth and the same encoder
//! settings, carrying nothing of the file it came from. Resolution, aspect ratio, compression
//! artefacts and a source's colour signature each single one image out of the eight shown, and a
//! forced-choice ESP experiment that leaks such a channel measures eyesight and reports psi. That
//! is the classic way these experiments produce false positives, and it is why this file exists.
//!
//! It matters at every pool size. A large pool does nothing about a target that is simply the
//! sharper picture.

use image::{ImageEncoder, ImageReader, imageops::FilterType};
use sha2::{Digest, Sha256};
use std::io::Cursor;

/// Edge length of every published image. Square, so aspect ratio stops being an observable at the
/// cost of the margins of a panorama — a crop is a curation loss, a ratio is a leak.
pub const EDGE: u32 = 512;

/// Bits kept per colour channel, giving 32 levels each.
///
/// Requantisation is uniform across the pool, which is the whole point: JPEG ringing, a scanner's
/// tonal curve and an 8-bit-per-channel photograph all arrive at the same quantisation grid, so
/// none of them marks its own image. Coarse enough to swallow those differences, fine enough that
/// a photograph still reads as one.
pub const CHANNEL_BITS: u8 = 5;

/// Hex characters of the digest kept in an identifier — 128 bits.
///
/// Identity, not a security binding: the full digest would double the width of every id in the
/// manifest and in every log entry that names a target, and 128 bits collides at a rate no pool
/// this project could reach will notice.
const ID_HEX: usize = 32;

pub struct Normalised {
    /// `img_` plus the hash of `png`, so identity follows content. A filename, a re-upload or a
    /// second copy under a different name cannot change what an identifier means.
    pub id: String,
    pub png: Vec<u8>,
}

#[derive(Debug)]
pub enum NormaliseError {
    Decode(String),
    Encode(String),
    /// Upscaling invents detail, and invented detail is exactly the kind of texture difference
    /// this module exists to remove.
    TooSmall {
        edge: u32,
    },
}

impl std::fmt::Display for NormaliseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NormaliseError::Decode(e) => write!(f, "cannot decode: {e}"),
            NormaliseError::Encode(e) => write!(f, "cannot encode: {e}"),
            NormaliseError::TooSmall { edge } => write!(
                f,
                "shortest edge is {edge}px, at least {EDGE}px is required — upscaling would invent \
                 detail the other seven images do not have"
            ),
        }
    }
}

impl std::error::Error for NormaliseError {}

/// Decode, centre-crop to a square, resize to [`EDGE`], flatten, requantise, re-encode as PNG.
///
/// The decoder hands back pixels and nothing else, so EXIF, ICC profiles, XMP, thumbnails and the
/// GPS coordinates of somebody's holiday never reach the output: stripping metadata is not a step
/// here, it is a consequence of only ever writing pixels we computed.
pub fn normalise(bytes: &[u8]) -> Result<Normalised, NormaliseError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| NormaliseError::Decode(e.to_string()))?;
    let img = reader
        .decode()
        .map_err(|e| NormaliseError::Decode(e.to_string()))?;

    let (w, h) = (img.width(), img.height());
    let side = w.min(h);
    if side < EDGE {
        return Err(NormaliseError::TooSmall { edge: side });
    }

    let square = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
    let scaled = square
        .resize_exact(EDGE, EDGE, FilterType::Lanczos3)
        .to_rgba8();

    let step = 8 - CHANNEL_BITS;
    let levels = (1u16 << CHANNEL_BITS) - 1;
    let mut flat = image::RgbImage::new(EDGE, EDGE);
    for (px, out) in scaled.pixels().zip(flat.pixels_mut()) {
        let a = px.0[3] as u16;
        for c in 0..3 {
            // Composite onto white rather than dropping alpha. A transparent PNG whose alpha was
            // merely discarded shows whatever happened to sit in the colour channels, which is a
            // difference between this image and the other seven and therefore a channel.
            let over = ((px.0[c] as u16 * a + 255 * (255 - a)) / 255) as u8;
            let q = (over >> step) as u16;
            // Back to the full range so the quantisation grid, not the source, sets the endpoints.
            out.0[c] = ((q * 255 + levels / 2) / levels) as u8;
        }
    }

    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(flat.as_raw(), EDGE, EDGE, image::ExtendedColorType::Rgb8)
        .map_err(|e| NormaliseError::Encode(e.to_string()))?;

    // Plain SHA-256 over one field: there is only one field, so nothing can be re-split, and a
    // curator checking a published file with `sha256sum` gets the identifier back.
    let digest = Sha256::digest(&png);
    let mut id = String::with_capacity(4 + ID_HEX);
    id.push_str("img_");
    for b in &digest[..ID_HEX / 2] {
        id.push_str(&format!("{b:02x}"));
    }

    Ok(Normalised { id, png })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source image with a recognisable per-pixel pattern, so a normalisation that ignored its
    /// input would not accidentally pass.
    fn source(w: u32, h: u32, tint: u8) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = image::Rgb([
                    (x * 255 / w) as u8,
                    (y * 255 / h) as u8,
                    tint.wrapping_add(((x / 32 + y / 32) * 24) as u8),
                ]);
                img.put_pixel(x, y, v);
            }
        }
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn decode(png: &[u8]) -> image::RgbImage {
        image::load_from_memory(png).unwrap().to_rgb8()
    }

    /// The leak this module exists to close: two images that differ only in shape must come out
    /// indistinguishable in shape.
    #[test]
    fn resolution_and_aspect_ratio_do_not_survive() {
        let a = normalise(&source(1600, 900, 0)).unwrap();
        let b = normalise(&source(600, 2000, 0)).unwrap();
        for n in [&a, &b] {
            let img = decode(&n.png);
            assert_eq!((img.width(), img.height()), (EDGE, EDGE));
        }
    }

    #[test]
    fn every_channel_lands_on_the_quantisation_grid() {
        let n = normalise(&source(900, 900, 17)).unwrap();
        let levels = (1u16 << CHANNEL_BITS) - 1;
        let allowed: Vec<u8> = (0..=levels)
            .map(|q| ((q * 255 + levels / 2) / levels) as u8)
            .collect();
        for px in decode(&n.png).pixels() {
            for c in 0..3 {
                assert!(
                    allowed.contains(&px.0[c]),
                    "value {} is off the grid",
                    px.0[c]
                );
            }
        }
    }

    /// Identity follows content. Re-running the pipeline on the same bytes must mint the same id,
    /// or `poolctl check` could never recognise a duplicate.
    #[test]
    fn the_identifier_is_stable_and_content_derived() {
        let src = source(700, 700, 3);
        let a = normalise(&src).unwrap();
        let b = normalise(&src).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.png, b.png);
        assert!(a.id.starts_with("img_"));
        assert_eq!(a.id.len(), 4 + ID_HEX);

        let other = normalise(&source(700, 700, 200)).unwrap();
        assert_ne!(a.id, other.id);
    }

    /// PNG chunk types, in order.
    fn chunks(png: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = 8; // signature
        while at + 8 <= png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            out.push(String::from_utf8_lossy(&png[at + 4..at + 8]).into_owned());
            at += 12 + len; // length, type, data, CRC
        }
        out
    }

    /// A minimal EXIF block spliced in after the start-of-image marker. Real files carry camera
    /// model, timestamps and GPS coordinates here.
    fn with_exif(jpeg: &[u8]) -> Vec<u8> {
        let payload: &[u8] = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\0";
        let len = (payload.len() + 2) as u16;
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1, (len >> 8) as u8, len as u8];
        out.extend_from_slice(payload);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    /// Metadata is not stripped by a step that could be forgotten — only decoded pixels are ever
    /// written — so it can neither reach a viewer nor move an identifier.
    #[test]
    fn source_metadata_reaches_neither_the_bytes_nor_the_identifier() {
        let mut jpeg = Vec::new();
        image::load_from_memory(&source(1024, 1024, 40))
            .unwrap()
            .write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();
        let tagged = with_exif(&jpeg);
        assert!(
            tagged.windows(4).any(|w| w == b"Exif"),
            "the fixture must actually carry EXIF"
        );

        let plain = normalise(&jpeg).unwrap();
        let annotated = normalise(&tagged).unwrap();
        assert_eq!(
            plain.id, annotated.id,
            "metadata must not be able to mint a second identity"
        );

        for n in [&plain, &annotated] {
            assert!(!n.png.windows(4).any(|w| w == b"Exif"));
            assert_eq!(chunks(&n.png), ["IHDR", "IDAT", "IEND"]);
        }
    }

    #[test]
    fn refuses_an_image_that_would_have_to_be_upscaled() {
        assert!(matches!(
            normalise(&source(400, 900, 0)),
            Err(NormaliseError::TooSmall { edge: 400 })
        ));
    }

    #[test]
    fn refuses_what_is_not_an_image() {
        assert!(matches!(
            normalise(b"this is not a photograph"),
            Err(NormaliseError::Decode(_))
        ));
    }
}
