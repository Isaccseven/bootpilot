use boot_core::*;
use uuid::Uuid;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadOption {
    pub description: String,
    pub partition_id: Option<Uuid>,
    pub path: Option<String>,
}
fn malformed() -> BootError {
    super::error(ErrorCode::VerificationFailed, "Ungültige UEFI-Laderdaten")
}
pub fn parse_u16(bytes: &[u8]) -> BootResult<u16> {
    let b: [u8; 2] = bytes.try_into().map_err(|_| malformed())?;
    Ok(u16::from_le_bytes(b))
}
pub fn parse_utf16(bytes: &[u8]) -> BootResult<String> {
    if !bytes.len().is_multiple_of(2) {
        return Err(malformed());
    }
    let mut values: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .collect();
    if values.last() == Some(&0) {
        values.pop();
    }
    if values.contains(&0) {
        return Err(malformed());
    }
    String::from_utf16(&values).map_err(|_| malformed())
}
pub fn parse_load_option(bytes: &[u8]) -> BootResult<LoadOption> {
    if bytes.len() < 8 || bytes.len() > 65536 {
        return Err(malformed());
    }
    let path_len = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    let end_description = (6..bytes.len() - 1)
        .step_by(2)
        .find(|&i| bytes[i..i + 2] == [0, 0])
        .ok_or_else(malformed)?;
    let description = parse_utf16(&bytes[6..end_description])?;
    let start = end_description + 2;
    let end = start
        .checked_add(path_len)
        .filter(|&v| v <= bytes.len())
        .ok_or_else(malformed)?;
    let mut cursor = start;
    let mut partition_id = None;
    let mut path = String::new();
    let mut terminated = false;
    while cursor < end {
        if cursor + 4 > end {
            return Err(malformed());
        }
        let node = &bytes[cursor..end];
        let length = u16::from_le_bytes([node[2], node[3]]) as usize;
        if length < 4 || cursor + length > end {
            return Err(malformed());
        }
        if node[0] == 4 && node[1] == 1 && length == 42 && node[41] == 2 {
            let raw: [u8; 16] = node[24..40].try_into().map_err(|_| malformed())?;
            partition_id = Some(Uuid::from_bytes_le(raw));
        }
        if node[0] == 4 && node[1] == 4 {
            path.push_str(&parse_utf16(&node[4..length])?);
        }
        if node[0] == 0x7f && node[1] == 0xff {
            if length != 4 || cursor + length != end {
                return Err(malformed());
            }
            terminated = true;
        }
        cursor += length;
    }
    if !terminated {
        return Err(malformed());
    }
    Ok(LoadOption {
        description,
        partition_id,
        path: if path.is_empty() { None } else { Some(path) },
    })
}
