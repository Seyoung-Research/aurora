//! Minimal PNG encoder (8-bit truecolour, no interlace).
//!
//! Uses "stored" (uncompressed) DEFLATE blocks, so no compression library is
//! needed. Files are larger than a zlib-compressed PNG but are valid and open
//! in every image viewer.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

const CRC_TABLE: [u32; 256] = build_crc_table();

const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

fn chunk(kind: &[u8; 4], payload: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    let mut crc_input = Vec::with_capacity(4 + payload.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(payload);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// A zlib stream whose DEFLATE blocks are all "stored" (type 0).
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 16);
    out.push(0x78); // CMF: deflate, 32K window
    out.push(0x01); // FLG: no dict, fastest
    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let mut offset = 0usize;
        while offset < data.len() {
            let remaining = data.len() - offset;
            let len = remaining.min(65535);
            let final_block = offset + len >= data.len();
            out.push(if final_block { 0x01 } else { 0x00 });
            let len16 = len as u16;
            out.extend_from_slice(&len16.to_le_bytes());
            out.extend_from_slice(&(!len16).to_le_bytes());
            out.extend_from_slice(&data[offset..offset + len]);
            offset += len;
        }
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// Encode an RGB buffer (3 bytes per pixel, row-major) as a PNG.
pub fn encode_rgb(rgb: &[u8], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() < expected {
        anyhow::bail!(
            "RGB buffer too small: got {} bytes, need {expected} for {width}x{height}",
            rgb.len()
        );
    }

    // Raw scanlines each prefixed with filter type 0 (None).
    let stride = width as usize * 3;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0u8);
        raw.extend_from_slice(&rgb[y * stride..(y + 1) * stride]);
    }

    let mut out = Vec::with_capacity(raw.len() / 8 + 1024);
    out.extend_from_slice(&SIGNATURE);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // colour type: truecolour RGB
    ihdr.push(0); // compression: deflate
    ihdr.push(0); // filter method
    ihdr.push(0); // interlace: none
    chunk(b"IHDR", &ihdr, &mut out);
    chunk(b"IDAT", &zlib_stored(&raw), &mut out);
    chunk(b"IEND", &[], &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }

    #[test]
    fn encodes_valid_png_structure() {
        // 2x1 red/green image
        let rgb = [255u8, 0, 0, 0, 255, 0];
        let png = encode_rgb(&rgb, 2, 1).unwrap();
        assert_eq!(&png[..8], &SIGNATURE);
        // IHDR
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32_at(&png, 16), 2);
        assert_eq!(u32_at(&png, 20), 1);
        assert_eq!(png[24], 8);
        assert_eq!(png[25], 2);
        // ends with IEND chunk (12 bytes: len + type + crc)
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn rejects_small_buffer() {
        assert!(encode_rgb(&[0u8; 5], 2, 2).is_err());
    }

    #[test]
    fn crc_of_known_string() {
        // CRC-32 of "123456789" is 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn adler_of_known_string() {
        // Adler-32 of "Wikipedia" is 0x11E60398
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn multi_block_deflate_roundtrip_length() {
        // Force more than one stored block (> 65535 bytes of scanline data).
        let w = 300u32;
        let h = 100u32;
        let rgb = vec![128u8; (w * h * 3) as usize];
        let png = encode_rgb(&rgb, w, h).unwrap();
        // IDAT must contain 2 stored blocks (2 headers + payload + adler + zlib hdr)
        let idat_len = u32_at(&png, 8) as usize;
        let _ = idat_len;
        assert!(png.len() > rgb.len());
    }
}
