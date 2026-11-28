use super::CompactDecodingError;

#[cfg(test)]
use super::CompactEncodingError;

fn read_array<const N: usize>(
    input: &mut &[u8]
) -> Result<[u8; N], CompactDecodingError> {
    let bytes = input
        .get(..N)
        .ok_or(CompactDecodingError::UnexpectedEnd)?;
    let array = bytes
        .try_into()
        .map_err(|_| CompactDecodingError::UnexpectedEnd)?;
    *input = &input[N..];
    Ok(array)
}

pub(super) fn write_u8(
    buffer: &mut Vec<u8>,
    value: u8,
) {
    buffer.push(value);
}

pub(super) fn write_u32(
    buffer: &mut Vec<u8>,
    value: u32,
) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn write_u64(
    buffer: &mut Vec<u8>,
    value: u64,
) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn write_i64(
    buffer: &mut Vec<u8>,
    value: i64,
) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn write_f64(
    buffer: &mut Vec<u8>,
    value: f64,
) {
    // Preserve the exact IEEE 754 bit pattern.
    write_u64(buffer, value.to_bits());
}

pub(super) fn write_bool(
    buffer: &mut Vec<u8>,
    value: bool,
) {
    write_u8(buffer, u8::from(value));
}

#[cfg(test)]
pub(super) fn write_string(
    buffer: &mut Vec<u8>,
    value: &str,
) -> Result<(), CompactEncodingError> {
    u32::try_from(value.len())
        .map_err(|_| CompactEncodingError::StringTooLong)?;
    write_string_unchecked(buffer, value);
    Ok(())
}

pub(super) fn write_string_unchecked(
    buffer: &mut Vec<u8>,
    value: &str,
) {
    let length = value.len() as u32;
    write_u32(buffer, length);
    buffer.extend_from_slice(value.as_bytes());
}

pub(super) fn read_u8(input: &mut &[u8]) -> Result<u8, CompactDecodingError> {
    Ok(read_array::<1>(input)?[0])
}

pub(super) fn read_u32(input: &mut &[u8]) -> Result<u32, CompactDecodingError> {
    Ok(u32::from_le_bytes(read_array::<4>(input)?))
}

pub(super) fn read_u64(input: &mut &[u8]) -> Result<u64, CompactDecodingError> {
    Ok(u64::from_le_bytes(read_array::<8>(input)?))
}

pub(super) fn read_i64(input: &mut &[u8]) -> Result<i64, CompactDecodingError> {
    Ok(i64::from_le_bytes(read_array::<8>(input)?))
}

pub(super) fn read_f64(input: &mut &[u8]) -> Result<f64, CompactDecodingError> {
    // Reconstruct the floating-point value from its bits.
    Ok(f64::from_bits(read_u64(input)?))
}

pub(super) fn read_bool(
    input: &mut &[u8]
) -> Result<bool, CompactDecodingError> {
    let mut remaining = *input;

    let value = match read_u8(&mut remaining)? {
        0 => false,
        1 => true,
        _ => return Err(CompactDecodingError::InvalidBoolean),
    };

    *input = remaining;
    Ok(value)
}

#[cfg(any(test, feature = "storage-validation"))]
pub(super) fn read_string<'a>(
    input: &mut &'a [u8]
) -> Result<&'a str, CompactDecodingError> {
    let mut remaining = *input;
    let length = read_u32(&mut remaining)? as usize;

    let bytes = remaining
        .get(..length)
        .ok_or(CompactDecodingError::UnexpectedEnd)?;

    let value = std::str::from_utf8(bytes)
        .map_err(|_| CompactDecodingError::InvalidUtf8)?;

    *input = &remaining[length..];
    Ok(value)
}

/// Read a string from bytes produced exclusively by our private encoder.
/// Length/bounds checks remain active even in optimized builds.
///
/// # Safety invariant
/// `write_string_unchecked` copies bytes from a valid `&str` and stores its
/// length; the ring stores complete encoded records and copies whole payloads
/// when growing. Reuse only space that no longer contains retained records. No API exposes mutable retained payload bytes.
/// If any of those properties change, this function must be revisited.
#[cfg(not(feature = "storage-validation"))]
pub(super) fn read_trusted_string<'a>(
    input: &mut &'a [u8]
) -> Result<&'a str, CompactDecodingError> {
    let mut remaining = *input;
    let length = read_u32(&mut remaining)? as usize;
    let bytes = remaining
        .get(..length)
        .ok_or(CompactDecodingError::UnexpectedEnd)?;

    // SAFETY: The private encoder only copies valid UTF-8 `&str` bytes.
    // The store never exposes mutable access to its encoded payloads.
    let value = unsafe { std::str::from_utf8_unchecked(bytes) };
    *input = &remaining[length..];
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_round_trip() {
        let mut buffer = Vec::new();
        write_u8(&mut buffer, 7);
        write_u32(&mut buffer, 0x12345678);
        write_u64(&mut buffer, u64::MAX);
        write_i64(&mut buffer, -42);
        write_f64(&mut buffer, -0.0);

        let mut input = buffer.as_slice();
        assert_eq!(read_u8(&mut input), Ok(7));
        assert_eq!(read_u32(&mut input), Ok(0x12345678));
        assert_eq!(read_u64(&mut input), Ok(u64::MAX));
        assert_eq!(read_i64(&mut input), Ok(-42));

        assert_eq!(
            read_f64(&mut input).unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );

        assert!(input.is_empty());
    }

    #[test]
    fn string_and_boolean_round_trip() {
        let mut buffer = Vec::new();
        write_string(&mut buffer, "café 猫").unwrap();
        write_bool(&mut buffer, false);
        write_bool(&mut buffer, true);

        let mut input = buffer.as_slice();
        assert_eq!(read_string(&mut input), Ok("café 猫"));
        assert_eq!(read_bool(&mut input), Ok(false));
        assert_eq!(read_bool(&mut input), Ok(true));
        assert!(input.is_empty());
    }

    #[test]
    fn invalid_boolean_preserves_cursor() {
        let bytes = [2, 42];
        let mut input = bytes.as_slice();

        assert_eq!(
            read_bool(&mut input),
            Err(CompactDecodingError::InvalidBoolean)
        );

        assert_eq!(input, bytes.as_slice());
    }

    #[test]
    fn truncated_number_preserves_cursor() {
        let bytes = [1, 2, 3];
        let mut input = bytes.as_slice();

        assert_eq!(
            read_u64(&mut input),
            Err(CompactDecodingError::UnexpectedEnd)
        );

        assert_eq!(input, bytes.as_slice());
    }

    #[test]
    fn invalid_utf8_preserves_cursor() {
        let bytes = [1, 0, 0, 0, 0xFF];
        let mut input = bytes.as_slice();

        assert_eq!(
            read_string(&mut input),
            Err(CompactDecodingError::InvalidUtf8)
        );

        assert_eq!(input, bytes.as_slice());
    }

    #[test]
    fn float_preserves_nan_bits() {
        let original = f64::from_bits(0x7ff8_0000_0000_0042);
        let mut buffer = Vec::new();
        write_f64(&mut buffer, original);

        let mut input = buffer.as_slice();
        assert_eq!(read_f64(&mut input).unwrap().to_bits(), original.to_bits());
    }
}
