//! Read bounded little-endian values and terminated strings from owned file sections.
//!
//! Imported from Foveonic's X3F reader.

use crate::input_error::damaged;
use crate::source::Result;

#[derive(Clone, Copy)]
pub(super) struct Bytes<'a>(pub(super) &'a [u8]);

impl<'a> Bytes<'a> {
    pub(super) fn range(self, offset: usize, length: usize) -> Result<&'a [u8]> {
        let end = offset
            .checked_add(length)
            .ok_or_else(|| damaged("X3F offset overflow"))?;
        self.0
            .get(offset..end)
            .ok_or_else(|| damaged("truncated X3F section"))
    }
    pub(super) fn u16(self, offset: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.range(offset, 2)?.try_into()?))
    }
    pub(super) fn u32(self, offset: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.range(offset, 4)?.try_into()?))
    }
    pub(super) fn offset(self, position: usize) -> Result<usize> {
        Ok(self.u32(position)? as usize)
    }
    pub(super) fn text(self, offset: usize) -> Result<String> {
        let tail = self
            .0
            .get(offset..)
            .ok_or_else(|| damaged("string outside X3F section"))?;
        let end = tail
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| damaged("unterminated X3F string"))?;
        Ok(std::str::from_utf8(&tail[..end])?.to_owned())
    }
    pub(super) fn utf16(self, offset: usize, remaining: &mut usize) -> Result<String> {
        let tail = self
            .0
            .get(offset..)
            .ok_or_else(|| damaged("string outside property section"))?;
        let units = tail.as_chunks::<2>().0;
        let end = units
            .iter()
            .position(|pair| *pair == [0, 0])
            .ok_or_else(|| damaged("unterminated UTF16 property"))?;
        // Shared offsets may reuse the same text many times. Charge each owned
        // string before growing it, rather than limiting only encoded bytes.
        let mut text = String::new();
        for character in char::decode_utf16(units[..end].iter().copied().map(u16::from_le_bytes)) {
            let character = character.map_err(|_| damaged("invalid UTF16 property"))?;
            *remaining = remaining
                .checked_sub(character.len_utf8())
                .ok_or_else(|| damaged("decoded X3F properties exceed limit"))?;
            text.push(character);
        }
        Ok(text)
    }
}

pub(super) fn product(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b).ok_or_else(|| damaged("X3F size overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_and_strings_stay_inside_their_section() {
        let bytes = Bytes(b"a\0b");
        assert_eq!(bytes.text(0).unwrap(), "a");
        assert!(bytes.text(2).is_err());
        assert!(bytes.range(usize::MAX, 2).is_err());
        assert!(bytes.u32(0).is_err());
        assert!(product(usize::MAX, 2).is_err());
        assert!(Bytes(&[0, 0xd8, 0, 0]).utf16(0, &mut 1024).is_err());
        assert!(Bytes(&[65, 0]).utf16(0, &mut 1024).is_err());
    }
}
