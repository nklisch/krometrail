use uuid::Uuid;

use crate::persistence_error;

pub(crate) struct WireReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> WireReader<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    pub(crate) fn read_u8(&mut self) -> krometrail_core::Result<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    pub(crate) fn read_u16(&mut self) -> krometrail_core::Result<u16> {
        Ok(u16::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_u32(&mut self) -> krometrail_core::Result<u32> {
        Ok(u32::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_u64(&mut self) -> krometrail_core::Result<u64> {
        Ok(u64::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_i128(&mut self) -> krometrail_core::Result<i128> {
        Ok(i128::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_f64(&mut self) -> krometrail_core::Result<f64> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    pub(crate) fn read_uuid(&mut self) -> krometrail_core::Result<Uuid> {
        Ok(Uuid::from_bytes(self.read_array()?))
    }

    pub(crate) fn read_bytes(&mut self, length: usize) -> krometrail_core::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| persistence_error("segment length overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| persistence_error("segment data ended before the declared length"))?;
        self.offset = end;
        Ok(value)
    }

    fn read_array<const N: usize>(&mut self) -> krometrail_core::Result<[u8; N]> {
        self.read_bytes(N)?
            .try_into()
            .map_err(|_| persistence_error("segment field has an invalid width"))
    }
}

pub(crate) fn put_uuid(output: &mut Vec<u8>, value: &Uuid) {
    output.extend_from_slice(value.as_bytes());
}

pub(crate) fn usize_from_u64(value: u64) -> krometrail_core::Result<usize> {
    usize::try_from(value).map_err(|_| persistence_error("segment length exceeds this platform"))
}
