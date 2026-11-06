//! 测试用的 RowBinary 解码，以及 ClickHouse HTTP 插入的 lz4 帧解压。
//!
//! 帧布局与 clickhouse 0.15.3 的 `decompress=1` 一致：16 字节 checksum、
//! 1 字节 magic（lz4 为 0x82）、u32 压缩长度（含 9 字节头）、u32 原始长度、
//! lz4 block。checksum 是 CityHash，mock 只核对 magic 和长度，不重算哈希。
//! 列顺序与 [`crate::MesEvent`] 一致。

use crate::event::MesEvent;

const LZ4_MAGIC: u8 = 0x82;
const CHECKSUM_LEN: usize = 16;
const HEADER_LEN: usize = 9;

pub fn frame_is_lz4(input: &[u8]) -> bool {
    input.len() > CHECKSUM_LEN && input[CHECKSUM_LEN] == LZ4_MAGIC
}

pub fn decompress_ch_lz4(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut offset = 0;
    if input.is_empty() {
        return Ok(out);
    }
    while offset < input.len() {
        if input.len() - offset < CHECKSUM_LEN + HEADER_LEN {
            return Err(format!("lz4 帧被截断，剩余 {} 字节", input.len() - offset));
        }
        let magic = input[offset + CHECKSUM_LEN];
        if magic != LZ4_MAGIC {
            return Err(format!("lz4 magic 不是 0x82，而是 {magic:#x}"));
        }
        let compressed_size = u32::from_le_bytes(
            input[offset + CHECKSUM_LEN + 1..offset + CHECKSUM_LEN + 5]
                .try_into()
                .map_err(|_| "compressed_size 越界")?,
        ) as usize;
        let uncompressed_size = u32::from_le_bytes(
            input[offset + CHECKSUM_LEN + 5..offset + CHECKSUM_LEN + 9]
                .try_into()
                .map_err(|_| "uncompressed_size 越界")?,
        ) as usize;
        if compressed_size < HEADER_LEN {
            return Err(format!("压缩长度 {compressed_size} 小于帧头"));
        }
        let frame_len = CHECKSUM_LEN + compressed_size;
        if input.len() - offset < frame_len {
            return Err("lz4 帧数据短于头里的长度".into());
        }
        let data_start = offset + CHECKSUM_LEN + HEADER_LEN;
        let data_end = offset + frame_len;
        let block = &input[data_start..data_end];
        if uncompressed_size == 0 {
            if !block.is_empty() {
                return Err("空块仍带有 lz4 数据".into());
            }
        } else {
            let decoded = lz4_block_decompress(block, uncompressed_size)?;
            out.extend_from_slice(&decoded);
        }
        offset += frame_len;
    }
    Ok(out)
}

pub fn decode_events(input: &[u8]) -> Result<Vec<MesEvent>, String> {
    let mut offset = 0;
    let mut rows = Vec::new();
    while offset < input.len() {
        rows.push(decode_one(input, &mut offset)?);
    }
    Ok(rows)
}

fn decode_one(input: &[u8], offset: &mut usize) -> Result<MesEvent, String> {
    Ok(MesEvent {
        work_order_id: read_string(input, offset)?,
        occurred_at: read_i64(input, offset)?,
        event_id: read_string(input, offset)?,
        line_id: read_string(input, offset)?,
        station_id: read_string(input, offset)?,
        kind: read_string(input, offset)?,
        quantity: read_u64(input, offset)?,
    })
}

fn read_string(input: &[u8], offset: &mut usize) -> Result<String, String> {
    let len = read_leb128(input, offset)?;
    let end = offset
        .checked_add(len)
        .ok_or_else(|| "字符串长度溢出".to_string())?;
    if end > input.len() {
        return Err("RowBinary 字符串被截断".into());
    }
    let bytes = &input[*offset..end];
    *offset = end;
    String::from_utf8(bytes.to_vec()).map_err(|err| format!("RowBinary 字符串不是 UTF-8: {err}"))
}

fn read_i64(input: &[u8], offset: &mut usize) -> Result<i64, String> {
    let bytes = read_array::<8>(input, offset)?;
    Ok(i64::from_le_bytes(bytes))
}

fn read_u64(input: &[u8], offset: &mut usize) -> Result<u64, String> {
    let bytes = read_array::<8>(input, offset)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_array<const N: usize>(input: &[u8], offset: &mut usize) -> Result<[u8; N], String> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| "整数读取溢出".to_string())?;
    if end > input.len() {
        return Err("RowBinary 整数被截断".into());
    }
    let mut buf = [0u8; N];
    buf.copy_from_slice(&input[*offset..end]);
    *offset = end;
    Ok(buf)
}

fn read_leb128(input: &[u8], offset: &mut usize) -> Result<usize, String> {
    let mut value: u64 = 0;
    let mut shift = 0;
    loop {
        if *offset >= input.len() {
            return Err("LEB128 被截断".into());
        }
        let byte = input[*offset];
        *offset += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return usize::try_from(value).map_err(|_| "LEB128 超出 usize".into());
        }
        shift += 7;
        if shift > 63 {
            return Err("LEB128 溢出".into());
        }
    }
}

/// lz4 block（不是 frame）。最后一段只有字面量，凑满 `expected` 即停。
fn lz4_block_decompress(src: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while out.len() < expected {
        if i >= src.len() {
            return Err(format!(
                "lz4 block 在 {}/{} 字节处结束",
                out.len(),
                expected
            ));
        }
        let token = src[i];
        i += 1;
        let mut lit_len = usize::from(token >> 4);
        if lit_len == 15 {
            lit_len += read_extra(src, &mut i)?;
        }
        if i + lit_len > src.len() {
            return Err("lz4 字面量越界".into());
        }
        out.extend_from_slice(&src[i..i + lit_len]);
        i += lit_len;
        if out.len() == expected {
            break;
        }
        if out.len() > expected {
            return Err("lz4 解出的字节超过帧头声明的长度".into());
        }
        if i + 2 > src.len() {
            return Err("lz4 match offset 被截断".into());
        }
        let match_offset = usize::from(u16::from_le_bytes([src[i], src[i + 1]]));
        i += 2;
        if match_offset == 0 || match_offset > out.len() {
            return Err(format!("lz4 match offset {match_offset} 非法"));
        }
        let mut match_len = usize::from(token & 0x0f) + 4;
        if token & 0x0f == 15 {
            match_len += read_extra(src, &mut i)?;
        }
        let start = out.len() - match_offset;
        for step in 0..match_len {
            let byte = out[start + step];
            out.push(byte);
            if out.len() == expected {
                break;
            }
        }
        if out.len() > expected {
            return Err("lz4 match 超过声明长度".into());
        }
    }
    if out.len() != expected {
        return Err(format!("lz4 解出 {} 字节，帧头声明 {expected}", out.len()));
    }
    Ok(out)
}

fn read_extra(src: &[u8], i: &mut usize) -> Result<usize, String> {
    let mut extra = 0usize;
    loop {
        if *i >= src.len() {
            return Err("lz4 长度扩展被截断".into());
        }
        let byte = src[*i];
        *i += 1;
        extra += usize::from(byte);
        if byte != 255 {
            return Ok(extra);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_only_frame_round_trips() {
        let payload = b"hello";
        let mut block = vec![0x50];
        block.extend_from_slice(payload);
        let mut frame = vec![0u8; 16];
        frame.push(LZ4_MAGIC);
        let compressed_size = u32::try_from(HEADER_LEN + block.len()).unwrap();
        frame.extend_from_slice(&compressed_size.to_le_bytes());
        frame.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
        frame.extend_from_slice(&block);
        assert!(frame_is_lz4(&frame));
        assert_eq!(decompress_ch_lz4(&frame).unwrap(), payload);
    }

    #[test]
    fn decodes_one_rowbinary_event() {
        let mut raw = Vec::new();
        push_str(&mut raw, "WO");
        raw.extend_from_slice(&1_700_000_000_000i64.to_le_bytes());
        push_str(&mut raw, "e1");
        push_str(&mut raw, "L");
        push_str(&mut raw, "S");
        push_str(&mut raw, "start");
        raw.extend_from_slice(&3u64.to_le_bytes());
        let rows = decode_events(&raw).unwrap();
        assert_eq!(
            rows,
            vec![MesEvent::new(
                "WO",
                1_700_000_000_000,
                "e1",
                "L",
                "S",
                "start",
                3
            )]
        );
    }

    fn push_str(buf: &mut Vec<u8>, text: &str) {
        assert!(text.len() < 128);
        buf.push(u8::try_from(text.len()).unwrap());
        buf.extend_from_slice(text.as_bytes());
    }
}
