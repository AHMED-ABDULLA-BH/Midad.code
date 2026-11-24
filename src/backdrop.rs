use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize};
use std::{fs, io::Cursor, path::Path, sync::Arc};
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Backdrop(pub Arc<String>);
impl<'de> Deserialize<'de> for Backdrop {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() > 5_500_000 {
            return Err(serde::de::Error::custom("صورة كبيرة"));
        }
        let bytes = STANDARD.decode(&s).map_err(serde::de::Error::custom)?;
        if image::guess_format(&bytes).ok() != Some(image::ImageFormat::Png) {
            return Err(serde::de::Error::custom("صورة غير صالحة"));
        }
        let (w, h) = image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::Png)
            .into_dimensions()
            .map_err(serde::de::Error::custom)?;
        if w.max(h) > 1800 {
            return Err(serde::de::Error::custom("أبعاد غير صالحة"));
        }
        image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(serde::de::Error::custom)?;
        Ok(Self(Arc::new(s)))
    }
}
pub fn load(path: &Path) -> Result<Backdrop, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 32_000_000 {
        return Err("الصورة أكبر من ٣٢ ميغابايت".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    from_bytes(&bytes, false)
}
pub fn from_bytes(bytes: &[u8], key_background: bool) -> Result<Backdrop, String> {
    let format = image::guess_format(bytes).map_err(|_| "نوع الصورة غير مدعوم")?;
    let (w, h) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    if u64::from(w) * u64::from(h) > 40_000_000 {
        return Err("أبعاد الصورة كبيرة جدًا".into());
    }
    let original = image::load_from_memory_with_format(bytes, format).map_err(|e| e.to_string())?;
    let mut original = original;
    if key_background && format == image::ImageFormat::Bmp && w > 0 && h > 0 {
        let mut rgba = original.into_rgba8();
        let key = *rgba.get_pixel(0, 0);
        for pixel in rgba.pixels_mut() {
            if *pixel == key {
                pixel[3] = 0;
            }
        }
        original = image::DynamicImage::ImageRgba8(rgba);
    }
    let mut image = if w.max(h) <= 1800 {
        original
    } else {
        original.thumbnail(1800, 1800)
    };
    loop {
        let mut out = Cursor::new(Vec::new());
        image
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        if out.get_ref().len() <= 4_000_000 {
            return Ok(Backdrop(Arc::new(STANDARD.encode(out.into_inner()))));
        }
        image = image.thumbnail(image.width() * 3 / 4, image.height() * 3 / 4);
    }
}
