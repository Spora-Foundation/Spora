use crate::{Address, AddressError, Prefix, Version};
use bech32::{
    primitives::checksum::{Engine, PackedFe32},
    Bech32m, ByteIterExt, Checksum, Fe32, Fe32IterExt,
};
use std::iter;

#[derive(Clone, PartialEq, Eq)]
pub struct Bech32mCodec<I>
where
    I: Iterator<Item = Fe32>,
{
    iter: I,
    checksum_remaining: usize,
    checksum_engine: Engine<Bech32m>,
}

impl<I> Bech32mCodec<I>
where
    I: Iterator<Item = Fe32>,
{
    #[inline]
    pub fn new(data: I) -> Self {
        Self { iter: data, checksum_remaining: Bech32m::CHECKSUM_LENGTH, checksum_engine: Engine::new() }
    }

    #[inline]
    pub fn input_fes<T>(&mut self, iter: T)
    where
        T: Iterator<Item = Fe32>,
    {
        for fe in iter {
            self.checksum_engine.input_fe(fe);
        }
    }
}

impl<I> Iterator for Bech32mCodec<I>
where
    I: Iterator<Item = Fe32>,
{
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(fe) => {
                self.checksum_engine.input_fe(fe);
                Some(fe.to_char())
            }
            None => {
                if self.checksum_remaining == 0 {
                    None
                } else {
                    if self.checksum_remaining == Bech32m::CHECKSUM_LENGTH {
                        self.checksum_engine.input_target_residue();
                    }
                    self.checksum_remaining -= 1;
                    let byte = self.checksum_engine.residue().unpack(self.checksum_remaining);
                    let fe32 = Fe32::try_from(byte).expect("unpacked checksum");
                    Some(fe32.to_char())
                }
            }
        }
    }
}

pub fn validate_checksum(prefix: &str, data: &str) -> Result<Vec<Fe32>, AddressError> {
    if data.len() > Bech32m::CODE_LENGTH {
        return Err(AddressError::InvalidAddress);
    }

    if data.len() < Bech32m::CHECKSUM_LENGTH {
        return Err(AddressError::BadChecksum);
    }

    let mut checksum_eng = Engine::<Bech32m>::new();
    for fe in prefix.bytes().bytes_to_fes() {
        checksum_eng.input_fe(fe);
    }

    let mut fes = Vec::with_capacity(data.len());
    for c in data.chars() {
        let fe = Fe32::from_char(c).map_err(|_| AddressError::DecodingError(c))?;
        checksum_eng.input_fe(fe);
        fes.push(fe);
    }

    if checksum_eng.residue() != &Bech32m::TARGET_RESIDUE {
        return Err(AddressError::BadChecksum);
    }

    Ok(fes)
}

impl Address {
    pub(crate) fn encode_payload(&self) -> String {
        let version = iter::once(self.version as u8);
        let payload = self.payload.iter().copied();
        let data = version.clone().chain(payload).bytes_to_fes();
        let mut encoder = Bech32mCodec::new(data);
        encoder.input_fes(self.prefix.as_str().bytes().bytes_to_fes());
        encoder.collect()
    }

    pub(crate) fn decode_payload(prefix: Prefix, address: &str) -> Result<Self, AddressError> {
        let mut fes = validate_checksum(prefix.as_str(), address)?;
        fes.truncate(address.len() - Bech32m::CHECKSUM_LENGTH);

        let data = fes.into_iter().fes_to_bytes().collect::<Vec<_>>();
        if data.is_empty() {
            return Err(AddressError::InvalidAddress);
        }
        let version = Version::try_from(data[0])?;
        let payload = &data[1..];

        Ok(Address::new(prefix, version, payload)?)
    }
}
