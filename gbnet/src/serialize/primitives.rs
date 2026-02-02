//! BitSerialize/BitDeserialize implementations for primitive types (u8..u64, i8..i64, f32, f64, bool).
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Read, Write};

use super::bit_io;
use super::{BitDeserialize, BitSerialize, ByteAlignedDeserialize, ByteAlignedSerialize};

macro_rules! impl_primitive_single_byte {
    ($($t:ty, $bits:expr, $write:ident, $read:ident),*) => {
        $(
            impl BitSerialize for $t {
                fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.write_bits(*self as u64, $bits)?;
                    Ok(())
                }
            }
            impl BitDeserialize for $t {
                fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
                    let value = reader.read_bits($bits)?;
                    Ok(value as $t)
                }
            }
            impl ByteAlignedSerialize for $t {
                fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.$write(*self)?;
                    Ok(())
                }
            }
            impl ByteAlignedDeserialize for $t {
                fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
                    let value = reader.$read()?;
                    Ok(value)
                }
            }
        )*
    };
}

macro_rules! impl_primitive_multi_byte {
    ($($t:ty, $bits:expr, $write:ident, $read:ident),*) => {
        $(
            impl BitSerialize for $t {
                fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.write_bits(*self as u64, $bits)?;
                    Ok(())
                }
            }
            impl BitDeserialize for $t {
                fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
                    let value = reader.read_bits($bits)?;
                    Ok(value as $t)
                }
            }
            impl ByteAlignedSerialize for $t {
                fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.$write::<LittleEndian>(*self)?;
                    Ok(())
                }
            }
            impl ByteAlignedDeserialize for $t {
                fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
                    let value = reader.$read::<LittleEndian>()?;
                    Ok(value)
                }
            }
        )*
    };
}

impl_primitive_single_byte!(u8, 8, write_u8, read_u8, i8, 8, write_i8, read_i8);

impl_primitive_multi_byte!(
    u16, 16, write_u16, read_u16, i16, 16, write_i16, read_i16, u32, 32, write_u32, read_u32, i32,
    32, write_i32, read_i32, u64, 64, write_u64, read_u64, i64, 64, write_i64, read_i64
);

impl BitSerialize for f32 {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_bits(self.to_bits() as u64, 32)?;
        Ok(())
    }
}

impl BitDeserialize for f32 {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
        let bits = reader.read_bits(32)? as u32;
        let value = f32::from_bits(bits);
        Ok(value)
    }
}

impl ByteAlignedSerialize for f32 {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        writer.write_f32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for f32 {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
        let value = reader.read_f32::<LittleEndian>()?;
        Ok(value)
    }
}

impl BitSerialize for f64 {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_bits(self.to_bits(), 64)?;
        Ok(())
    }
}

impl BitDeserialize for f64 {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
        let bits = reader.read_bits(64)?;
        let value = f64::from_bits(bits);
        Ok(value)
    }
}

impl ByteAlignedSerialize for f64 {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        writer.write_f64::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for f64 {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
        let value = reader.read_f64::<LittleEndian>()?;
        Ok(value)
    }
}

impl BitSerialize for bool {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_bit(*self)?;
        Ok(())
    }
}

impl BitDeserialize for bool {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        let value = reader.read_bit()?;
        Ok(value)
    }
}

impl ByteAlignedSerialize for bool {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u8(if *self { 1 } else { 0 })?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for bool {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        let value = reader.read_u8()?;
        Ok(value != 0)
    }
}
