// KLX sample files store logical text separately from the font-specific glyph strings.
// Both packet prefixes use the same bit permutation, with opposite nibble polarity.
use crate::document::MAX_CHARS;

pub(super) fn unpack(field: &[u8]) -> Result<Vec<u8>, String> {
    if !field.len().is_multiple_of(2) || field.len() > (MAX_CHARS * 2 + 4) * 2 {
        return Err("طول حقل النص في KLX غير مدعوم".into());
    }
    let units = field
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]));
    let s = String::from_utf16(&units.collect::<Vec<_>>()).map_err(|_| "نص KLX تالف")?;
    let encoded = s
        .strip_prefix("??=@")
        .map(|v| (v, 0))
        .or_else(|| s.strip_prefix("$*&!").map(|v| (v, 255)));
    if let Some((payload, mask)) = encoded {
        if !payload.len().is_multiple_of(2) || !payload.bytes().all(|b| (b'a'..=b'p').contains(&b))
        {
            return Err("ترميز نص KLX غير صالح".into());
        }
        return Ok(payload
            .as_bytes()
            .chunks_exact(2)
            .map(|p| {
                let byte = (((p[0] - b'a') << 4) | (p[1] - b'a')) ^ mask;
                (byte & 0xc3) | ((byte & 0x0c) << 2) | ((byte & 0x30) >> 2)
            })
            .collect());
    }
    // Some writers widened each byte directly; others used the Windows-1256 code page.
    const WINDOWS_1256: &str = concat!(
        "€پ‚ƒ„…†‡ˆ‰ٹ‹Œچژڈ",
        "گ‘’“”•–—ک™ڑ›œ\u{200c}\u{200d}ں",
        "\u{a0}،¢£¤¥¦§¨©ھ«¬\u{ad}®¯",
        "°±²³´µ¶·¸¹؛»¼½¾؟",
        "ہءآأؤإئابةتثجحخد",
        "ذرزسشصض×طظعغـفقك",
        "àلâمنهوçèéêëىيîï",
        "ًٌٍَôُِ÷ّùْûü\u{200e}\u{200f}ے",
    );
    let bytes: Vec<_> = s
        .chars()
        .map(|c| {
            u8::try_from(u32::from(c))
                .ok()
                .or_else(|| {
                    WINDOWS_1256
                        .chars()
                        .position(|v| v == c)
                        .map(|n| n as u8 + 128)
                })
                .unwrap_or(0)
        })
        .collect();
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            32..=126 => char::from(b),
            0xa1 | 0xc3 => 'ء',
            0xc1 => 'آ',
            0xc2 => 'ا',
            0xc4 => 'أ',
            0xc6 => 'إ',
            0xc7 => 'ئ',
            0xc8 => 'ب',
            0xc9 => 'پ',
            0xca => 'ت',
            0xcb => 'ة',
            0xcc => 'ث',
            0xcd => 'ج',
            0xce => 'چ',
            0xcf => 'ح',
            0xd0 => 'خ',
            0xd1 => 'د',
            0xd2 => 'ذ',
            0xd3 => 'ر',
            0xd4 => 'ز',
            0xd6 => 'س',
            0xd8 => 'ش',
            0xd9 => 'ص',
            0xda => 'ض',
            0xdb => 'ط',
            0xdc => 'ـ',
            0xdd => 'ظ',
            0xde => 'ع',
            0xdf => 'غ',
            0xe0 => 'ف',
            0xe1 => 'ق',
            0xe2 => 'ک',
            0xe3 => 'گ',
            0xe4 => 'ل',
            0xe5 => 'م',
            0xe6 => 'ن',
            0xe7 => 'و',
            0xe8 => 'ه',
            0xe9 => 'ی',
            0xea => 'ى',
            0xf0 => 'ً',
            0xf1 => 'ٌ',
            0xf3 => 'َ',
            0xf5 => 'ُ',
            0xf6 => 'ِ',
            0xf8 => 'ّ',
            0xfa => 'ْ',
            _ => char::REPLACEMENT_CHARACTER,
        })
        .collect()
}

pub(super) fn placeholder(bytes: &[u8]) -> bool {
    bytes.is_empty() || matches!(bytes, b"$*&!" | b"??=@")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }
    #[test]
    fn reads_both_packets_and_widened_legacy_strings() {
        for (input, expected) in [
            ("??=@mcninioi", "الله"),
            ("$*&!dnchchbh", "الله"),
            ("ÂåÊÏÂæ", "امتحان"),
            ("آهتدآو", "امتحان"),
            (
                "$*&!bpcjcgphdnchchbhphdnchdiamcgcfphdnchdiambgcg",
                "بسم الله الرحمن الرحیم",
            ),
        ] {
            assert_eq!(decode(&unpack(&field(input)).unwrap()), expected);
        }
        assert_eq!(decode(&[0xc4, 0xf3, 0xde, 0xf5, 0xe7, 0xd2, 0xf5]), "أَعُوذُ");
        assert_eq!(decode(&[0xa7]), "�");
        for input in ["??=@a", "$*&!aq"] {
            assert!(unpack(&field(input)).is_err());
        }
        assert!(unpack(&[0]).is_err());
    }
}
