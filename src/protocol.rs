use anyhow::Result;
use std::io::{Read, Write};

/// 长度前缀帧协议: [4 字节大端长度][UTF-8 payload]
pub fn write_frame(stream: &mut impl Write, payload: &str) -> Result<()> {
    let bytes = payload.as_bytes();
    let len = bytes.len() as u32;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()?;
    Ok(())
}

pub fn read_frame(stream: &mut impl Read) -> Result<String> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}
