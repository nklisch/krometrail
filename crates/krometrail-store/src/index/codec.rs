use uuid::Uuid;

use crate::persistence_error;

pub(crate) fn id(value: &Uuid) -> [u8; 16] {
    *value.as_bytes()
}

pub(crate) fn decode_id(value: &[u8]) -> krometrail_core::Result<Uuid> {
    Uuid::from_slice(value).map_err(|_| persistence_error("stored identifier is malformed"))
}

pub(crate) const fn u64_blob(value: u64) -> [u8; 8] {
    value.to_be_bytes()
}

pub(crate) fn decode_u64(value: &[u8]) -> krometrail_core::Result<u64> {
    let bytes: [u8; 8] = value
        .try_into()
        .map_err(|_| persistence_error("stored unsigned value is malformed"))?;
    Ok(u64::from_be_bytes(bytes))
}

pub(crate) const fn i128_blob(value: i128) -> [u8; 16] {
    value.to_be_bytes()
}

pub(crate) fn decode_i128(value: &[u8]) -> krometrail_core::Result<i128> {
    let bytes: [u8; 16] = value
        .try_into()
        .map_err(|_| persistence_error("stored signed value is malformed"))?;
    Ok(i128::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_values_round_trip() {
        let uuid = Uuid::from_u128(u128::MAX);
        assert_eq!(decode_id(&id(&uuid)).unwrap(), uuid);
        for value in [0, 1, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX] {
            assert_eq!(decode_u64(&u64_blob(value)).unwrap(), value);
        }
        for value in [i128::MIN, -1, 0, 1, i128::MAX] {
            assert_eq!(decode_i128(&i128_blob(value)).unwrap(), value);
        }
    }

    #[test]
    fn unsigned_blob_order_matches_numeric_order() {
        let values = [0, 1, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX];
        let mut encoded: Vec<_> = values.into_iter().map(u64_blob).collect();
        encoded.sort();
        let decoded: Vec<_> = encoded
            .iter()
            .map(|value| decode_u64(value).unwrap())
            .collect();
        assert_eq!(decoded, values);
    }
}
