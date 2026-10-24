//! A tiny deterministic zip writer (method 0 = stored). Data packs are small text files, and
//! Minecraft accepts stored entries, so this avoids a compression dependency.
//! Layout per the format's APPNOTE: local headers + data, central directory, end record.

use std::collections::BTreeMap;

fn crc32(data: &[u8]) -> u32 {
    // Table is cheap to build and avoids a dependency.
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = !0u32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Entries are written in map (sorted) order with a fixed 1980-01-01 timestamp, so the same
/// pack always produces the same bytes. Panics if a size or the entry count exceeds the
/// classic (non-zip64) limits, which a data pack won't reach.
pub fn write_zip(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    const FLAG_UTF8: u16 = 1 << 11;
    const DOS_TIME: u16 = 0;
    const DOS_DATE: u16 = (1 << 5) | 1; // 1980-01-01

    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = u32::try_from(out.len()).expect("zip too large");
        let size = u32::try_from(data.len()).expect("file too large for zip");
        let crc = crc32(data);
        let name_len = u16::try_from(name.len()).expect("file name too long");

        // local file header
        u32le(&mut out, 0x0403_4b50);
        u16le(&mut out, 10); // version needed
        u16le(&mut out, FLAG_UTF8);
        u16le(&mut out, 0); // stored
        u16le(&mut out, DOS_TIME);
        u16le(&mut out, DOS_DATE);
        u32le(&mut out, crc);
        u32le(&mut out, size);
        u32le(&mut out, size);
        u16le(&mut out, name_len);
        u16le(&mut out, 0); // extra length
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        // central directory header
        u32le(&mut central, 0x0201_4b50);
        u16le(&mut central, 20); // version made by
        u16le(&mut central, 10); // version needed
        u16le(&mut central, FLAG_UTF8);
        u16le(&mut central, 0);
        u16le(&mut central, DOS_TIME);
        u16le(&mut central, DOS_DATE);
        u32le(&mut central, crc);
        u32le(&mut central, size);
        u32le(&mut central, size);
        u16le(&mut central, name_len);
        u16le(&mut central, 0); // extra
        u16le(&mut central, 0); // comment
        u16le(&mut central, 0); // disk number
        u16le(&mut central, 0); // internal attrs
        u32le(&mut central, 0); // external attrs
        u32le(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }

    let cd_offset = u32::try_from(out.len()).expect("zip too large");
    let cd_size = u32::try_from(central.len()).expect("zip too large");
    let count = u16::try_from(files.len()).expect("too many files for zip");
    out.extend_from_slice(&central);
    u32le(&mut out, 0x0605_4b50);
    u16le(&mut out, 0);
    u16le(&mut out, 0);
    u16le(&mut out, count);
    u16le(&mut out, count);
    u32le(&mut out, cd_size);
    u32le(&mut out, cd_offset);
    u16le(&mut out, 0); // comment length
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn structure() {
        let mut files = BTreeMap::new();
        files.insert("a.txt".to_owned(), b"hello".to_vec());
        files.insert("d/b.txt".to_owned(), b"world".to_vec());
        let z = write_zip(&files);
        assert_eq!(&z[..4], &0x0403_4b50u32.to_le_bytes());
        assert_eq!(&z[z.len() - 22..z.len() - 18], &0x0605_4b50u32.to_le_bytes());
        // deterministic
        assert_eq!(z, write_zip(&files));
    }
}
