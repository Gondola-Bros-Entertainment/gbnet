//! BitSerialize/BitDeserialize implementations for collection types (String, Vec, Option, tuples, arrays).
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use log::debug;
use std::io::{self, Read, Write};

use super::bit_io;
use super::{BitDeserialize, BitSerialize, ByteAlignedDeserialize, ByteAlignedSerialize};

impl BitSerialize for String {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits for length
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (u64::BITS - (max_len as u64).leading_zeros()) as usize;

        if self.len() > max_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("String length {} exceeds max_len {}", self.len(), max_len),
            ));
        }

        writer.write_bits(self.len() as u64, len_bits)?;
        for byte in self.as_bytes() {
            writer.write_bits(*byte as u64, 8)?;
        }
        Ok(())
    }
}

impl BitDeserialize for String {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits for length
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (u64::BITS - (max_len as u64).leading_zeros()) as usize;
        let len = reader.read_bits(len_bits)? as usize;

        if len > max_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("String length {} exceeds max_len {}", len, max_len),
            ));
        }

        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(reader.read_bits(8)? as u8);
        }

        String::from_utf8(bytes).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {}", e))
        })
    }
}

impl ByteAlignedSerialize for String {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<LittleEndian>(self.len() as u32)?;
        writer.write_all(self.as_bytes())?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for String {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_u32::<LittleEndian>()? as usize;
        let mut bytes = vec![0u8; len];
        reader.read_exact(&mut bytes)?;

        String::from_utf8(bytes).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {}", e))
        })
    }
}

macro_rules! impl_array {
    ($($n:expr),*) => {
        $(
            impl<T: BitSerialize> BitSerialize for [T; $n] {
                fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
                    for item in self.iter() {
                        item.bit_serialize(writer)?;
                    }
                    Ok(())
                }
            }

            impl<T: BitDeserialize + Default + Copy> BitDeserialize for [T; $n] {
                fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
                    let mut array = [T::default(); $n];
                    for i in 0..$n {
                        array[i] = T::bit_deserialize(reader)?;
                    }
                    Ok(array)
                }
            }

            impl<T: ByteAlignedSerialize> ByteAlignedSerialize for [T; $n] {
                fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
                    for item in self.iter() {
                        item.byte_aligned_serialize(writer)?;
                    }
                    Ok(())
                }
            }

            impl<T: ByteAlignedDeserialize + Default + Copy> ByteAlignedDeserialize for [T; $n] {
                fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
                    let mut array = [T::default(); $n];
                    for i in 0..$n {
                        array[i] = T::byte_aligned_deserialize(reader)?;
                    }
                    Ok(array)
                }
            }
        )*
    };
}

impl_array!(
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 20, 24, 32, 48, 64, 96, 128, 256, 512,
    1024
);

impl<T: BitSerialize, U: BitSerialize> BitSerialize for (T, U) {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        self.0.bit_serialize(writer)?;
        self.1.bit_serialize(writer)?;
        Ok(())
    }
}

impl<T: BitDeserialize, U: BitDeserialize> BitDeserialize for (T, U) {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        Ok((T::bit_deserialize(reader)?, U::bit_deserialize(reader)?))
    }
}

impl<T: ByteAlignedSerialize, U: ByteAlignedSerialize> ByteAlignedSerialize for (T, U) {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
        self.0.byte_aligned_serialize(writer)?;
        self.1.byte_aligned_serialize(writer)?;
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize, U: ByteAlignedDeserialize> ByteAlignedDeserialize for (T, U) {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        Ok((
            T::byte_aligned_deserialize(reader)?,
            U::byte_aligned_deserialize(reader)?,
        ))
    }
}

impl<T: BitSerialize, U: BitSerialize, V: BitSerialize> BitSerialize for (T, U, V) {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        self.0.bit_serialize(writer)?;
        self.1.bit_serialize(writer)?;
        self.2.bit_serialize(writer)?;
        Ok(())
    }
}

impl<T: BitDeserialize, U: BitDeserialize, V: BitDeserialize> BitDeserialize for (T, U, V) {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        Ok((
            T::bit_deserialize(reader)?,
            U::bit_deserialize(reader)?,
            V::bit_deserialize(reader)?,
        ))
    }
}

impl<T: ByteAlignedSerialize, U: ByteAlignedSerialize, V: ByteAlignedSerialize> ByteAlignedSerialize
    for (T, U, V)
{
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
        self.0.byte_aligned_serialize(writer)?;
        self.1.byte_aligned_serialize(writer)?;
        self.2.byte_aligned_serialize(writer)?;
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize, U: ByteAlignedDeserialize, V: ByteAlignedDeserialize>
    ByteAlignedDeserialize for (T, U, V)
{
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        Ok((
            T::byte_aligned_deserialize(reader)?,
            U::byte_aligned_deserialize(reader)?,
            V::byte_aligned_deserialize(reader)?,
        ))
    }
}

impl<T: BitSerialize, U: BitSerialize, V: BitSerialize, W: BitSerialize> BitSerialize
    for (T, U, V, W)
{
    fn bit_serialize<Wr: bit_io::BitWrite>(&self, writer: &mut Wr) -> io::Result<()> {
        self.0.bit_serialize(writer)?;
        self.1.bit_serialize(writer)?;
        self.2.bit_serialize(writer)?;
        self.3.bit_serialize(writer)?;
        Ok(())
    }
}

impl<T: BitDeserialize, U: BitDeserialize, V: BitDeserialize, W: BitDeserialize> BitDeserialize
    for (T, U, V, W)
{
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        Ok((
            T::bit_deserialize(reader)?,
            U::bit_deserialize(reader)?,
            V::bit_deserialize(reader)?,
            W::bit_deserialize(reader)?,
        ))
    }
}

impl<
        T: ByteAlignedSerialize,
        U: ByteAlignedSerialize,
        V: ByteAlignedSerialize,
        W: ByteAlignedSerialize,
    > ByteAlignedSerialize for (T, U, V, W)
{
    fn byte_aligned_serialize<Wr: Write + WriteBytesExt>(&self, writer: &mut Wr) -> io::Result<()> {
        self.0.byte_aligned_serialize(writer)?;
        self.1.byte_aligned_serialize(writer)?;
        self.2.byte_aligned_serialize(writer)?;
        self.3.byte_aligned_serialize(writer)?;
        Ok(())
    }
}

impl<
        T: ByteAlignedDeserialize,
        U: ByteAlignedDeserialize,
        V: ByteAlignedDeserialize,
        W: ByteAlignedDeserialize,
    > ByteAlignedDeserialize for (T, U, V, W)
{
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        Ok((
            T::byte_aligned_deserialize(reader)?,
            U::byte_aligned_deserialize(reader)?,
            V::byte_aligned_deserialize(reader)?,
            W::byte_aligned_deserialize(reader)?,
        ))
    }
}

impl<T: BitSerialize> BitSerialize for Vec<T> {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (u64::BITS - (max_len as u64).leading_zeros()) as usize;
        if self.len() > max_len {
            debug!(
                "Error: Vector length {} exceeds max_len {}",
                self.len(),
                max_len
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Vector length {} exceeds max_len {}", self.len(), max_len),
            ));
        }
        writer.write_bits(self.len() as u64, len_bits)?;
        for item in self.iter() {
            item.bit_serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: BitDeserialize> BitDeserialize for Vec<T> {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (u64::BITS - (max_len as u64).leading_zeros()) as usize;
        let len = reader.read_bits(len_bits)? as usize;
        if len > max_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Vector length {} exceeds max_len {}", len, max_len),
            ));
        }
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::bit_deserialize(reader)?);
        }
        Ok(vec)
    }
}

impl<T: ByteAlignedSerialize> ByteAlignedSerialize for Vec<T> {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<LittleEndian>(self.len() as u32)?;
        for item in self.iter() {
            item.byte_aligned_serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize> ByteAlignedDeserialize for Vec<T> {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_u32::<LittleEndian>()? as usize;
        debug!("Deserialized Vec<T> length: {}", len);
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::byte_aligned_deserialize(reader)?);
        }
        Ok(vec)
    }
}

impl<T: BitSerialize> BitSerialize for Option<T> {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        match self {
            Some(value) => {
                writer.write_bit(true)?; // 1 bit for Some
                value.bit_serialize(writer)?;
            }
            None => {
                writer.write_bit(false)?; // 1 bit for None
            }
        }
        Ok(())
    }
}

impl<T: BitDeserialize> BitDeserialize for Option<T> {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
        let has_value = reader.read_bit()?;
        if has_value {
            Ok(Some(T::bit_deserialize(reader)?))
        } else {
            Ok(None)
        }
    }
}

impl<T: ByteAlignedSerialize> ByteAlignedSerialize for Option<T> {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        match self {
            Some(value) => {
                writer.write_u8(1)?;
                value.byte_aligned_serialize(writer)?;
            }
            None => {
                writer.write_u8(0)?;
            }
        }
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize> ByteAlignedDeserialize for Option<T> {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
        let has_value = reader.read_u8()? != 0;
        if has_value {
            Ok(Some(T::byte_aligned_deserialize(reader)?))
        } else {
            Ok(None)
        }
    }
}
