use crate::{
    prelude::{verify::TxErr, *},
    serializer::*,
};
use std::{
    io::{self, Cursor, Error},
    mem,
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// Max value is reserved for subscription updates or deserialization errors that occur during request processing.
    /// When a request is received with a reserved id, an IO error is returned regardless if the request is valid.
    pub id: u32,
    pub body: RequestBody,
}

impl Request {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push_u32(self.id);
        self.body.serialize(buf);
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let id = cursor.take_u32()?;
        let body = RequestBody::deserialize(cursor)?;
        Ok(Self { id, body })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    /// Max value represents a subscription update or an IO error during processing the request.
    pub id: u32,
    pub body: ResponseBody,
}

impl Response {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push_u32(self.id);
        self.body.serialize(buf);
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let id = cursor.take_u32()?;
        let body = ResponseBody::deserialize(cursor)?;
        Ok(Self { id, body })
    }
}

#[repr(u8)]
pub enum BodyType {
    // Returned to clients when an error occurred processing a request
    Error = 0x01,

    // Operations that can update the connection or blockchain state
    Broadcast = 0x10,
    SetBlockFilter = 0x11,
    ClearBlockFilter = 0x12,
    /// Subscribe to receive block updates. Any block filters applied may be ignored.
    Subscribe = 0x13,
    /// Unsubscribe from receiving block updates.
    Unsubscribe = 0x14,

    // Getters
    GetProperties = 0x20,
    GetBlock = 0x21,
    GetFullBlock = 0x22,
    GetBlockRange = 0x23,
    GetAddressInfo = 0x24,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RequestBody {
    Broadcast(TxVariant),
    SetBlockFilter(BlockFilter),
    ClearBlockFilter,
    Subscribe,
    Unsubscribe,
    GetProperties,
    GetBlock(u64),           // height
    GetFullBlock(u64),       // height
    GetBlockRange(u64, u64), // min height, max height
    GetAddressInfo(ScriptHash),
}

impl RequestBody {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Broadcast(tx) => {
                buf.reserve_exact(4096);
                buf.push(BodyType::Broadcast as u8);
                tx.serialize(buf);
            }
            Self::SetBlockFilter(filter) => {
                buf.reserve_exact(1 + (filter.len() * mem::size_of::<ScriptHash>()));
                buf.push(BodyType::SetBlockFilter as u8);
                buf.push(filter.len() as u8);
                for addr in filter {
                    buf.push_digest(&addr.0);
                }
            }
            Self::ClearBlockFilter => buf.push(BodyType::ClearBlockFilter as u8),
            Self::Subscribe => buf.push(BodyType::Subscribe as u8),
            Self::Unsubscribe => buf.push(BodyType::Unsubscribe as u8),
            Self::GetProperties => buf.push(BodyType::GetProperties as u8),
            Self::GetBlock(height) => {
                buf.reserve_exact(9);
                buf.push(BodyType::GetBlock as u8);
                buf.push_u64(*height);
            }
            Self::GetFullBlock(height) => {
                buf.reserve_exact(9);
                buf.push(BodyType::GetFullBlock as u8);
                buf.push_u64(*height);
            }
            Self::GetBlockRange(min_height, max_height) => {
                buf.reserve_exact(1 + (2 * mem::size_of::<u64>()));
                buf.push(BodyType::GetBlockRange as u8);
                buf.push_u64(*min_height);
                buf.push_u64(*max_height);
            }
            Self::GetAddressInfo(addr) => {
                buf.reserve_exact(33);
                buf.push(BodyType::GetAddressInfo as u8);
                buf.push_digest(&addr.0);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == BodyType::Broadcast as u8 => {
                let tx = TxVariant::deserialize(cursor)
                    .ok_or_else(|| Error::new(io::ErrorKind::InvalidData, "failed to decode tx"))?;
                Ok(Self::Broadcast(tx))
            }
            t if t == BodyType::SetBlockFilter as u8 => {
                let addr_len = usize::from(cursor.take_u8()?);
                let mut filter = BlockFilter::new();
                for _ in 0..addr_len {
                    filter.insert(ScriptHash(cursor.take_digest()?));
                }
                Ok(Self::SetBlockFilter(filter))
            }
            t if t == BodyType::ClearBlockFilter as u8 => Ok(Self::ClearBlockFilter),
            t if t == BodyType::Subscribe as u8 => Ok(Self::Subscribe),
            t if t == BodyType::Unsubscribe as u8 => Ok(Self::Unsubscribe),
            t if t == BodyType::GetProperties as u8 => Ok(Self::GetProperties),
            t if t == BodyType::GetBlock as u8 => {
                let height = cursor.take_u64()?;
                Ok(Self::GetBlock(height))
            }
            t if t == BodyType::GetFullBlock as u8 => {
                let height = cursor.take_u64()?;
                Ok(Self::GetFullBlock(height))
            }
            t if t == BodyType::GetBlockRange as u8 => {
                let min_height = cursor.take_u64()?;
                let max_height = cursor.take_u64()?;
                Ok(Self::GetBlockRange(min_height, max_height))
            }
            t if t == BodyType::GetAddressInfo as u8 => {
                let addr = ScriptHash(cursor.take_digest()?);
                Ok(Self::GetAddressInfo(addr))
            }
            _ => Err(Error::new(
                io::ErrorKind::InvalidData,
                "invalid msg request",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResponseBody {
    Error(ErrorKind),
    Broadcast,
    SetBlockFilter,
    ClearBlockFilter,
    Subscribe,
    Unsubscribe,
    GetProperties(Properties),
    GetBlock(FilteredBlock),
    GetFullBlock(Arc<Block>),
    GetBlockRange,
    GetAddressInfo(AddressInfo),
}

impl ResponseBody {
    pub fn is_err(&self) -> bool {
        match self {
            Self::Error(..) => true,
            _ => false,
        }
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Error(err) => {
                buf.reserve_exact(2048);
                buf.push(BodyType::Error as u8);
                err.serialize(buf);
            }
            Self::Broadcast => buf.push(BodyType::Broadcast as u8),
            Self::SetBlockFilter => buf.push(BodyType::SetBlockFilter as u8),
            Self::ClearBlockFilter => buf.push(BodyType::ClearBlockFilter as u8),
            Self::Subscribe => buf.push(BodyType::Subscribe as u8),
            Self::Unsubscribe => buf.push(BodyType::Unsubscribe as u8),
            Self::GetProperties(props) => {
                buf.reserve_exact(4096 + mem::size_of::<Properties>());
                buf.push(BodyType::GetProperties as u8);
                buf.push_u64(props.height);
                {
                    let mut tx_buf = Vec::with_capacity(4096);
                    props.owner.serialize(&mut tx_buf);
                    buf.extend_from_slice(&tx_buf);
                }
                buf.push_asset(props.network_fee);
                buf.push_asset(props.token_supply);
            }
            Self::GetBlock(block) => {
                buf.reserve_exact(1_048_576);
                buf.push(BodyType::GetBlock as u8);
                match block {
                    FilteredBlock::Header((header, signer)) => {
                        buf.push(0);
                        header.serialize(buf);
                        buf.push_sig_pair(signer);
                    }
                    FilteredBlock::Block(block) => {
                        buf.push(1);
                        block.serialize(buf);
                    }
                }
            }
            Self::GetFullBlock(block) => {
                buf.reserve_exact(1_048_576);
                buf.push(BodyType::GetFullBlock as u8);
                block.serialize(buf);
            }
            Self::GetBlockRange => buf.push(BodyType::GetBlockRange as u8),
            Self::GetAddressInfo(info) => {
                buf.reserve_exact(1 + (mem::size_of::<Asset>() * 3));
                buf.push(BodyType::GetAddressInfo as u8);
                buf.push_asset(info.net_fee);
                buf.push_asset(info.addr_fee);
                buf.push_asset(info.balance);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == BodyType::Error as u8 => {
                let err = ErrorKind::deserialize(cursor)?;
                Ok(Self::Error(err))
            }
            t if t == BodyType::Broadcast as u8 => Ok(Self::Broadcast),
            t if t == BodyType::SetBlockFilter as u8 => Ok(Self::SetBlockFilter),
            t if t == BodyType::ClearBlockFilter as u8 => Ok(Self::ClearBlockFilter),
            t if t == BodyType::Subscribe as u8 => Ok(Self::Subscribe),
            t if t == BodyType::Unsubscribe as u8 => Ok(Self::Unsubscribe),
            t if t == BodyType::GetProperties as u8 => {
                let height = cursor.take_u64()?;
                let owner = {
                    let tx = TxVariant::deserialize(cursor).ok_or_else(|| {
                        Error::new(io::ErrorKind::InvalidData, "failed to deserialize owner tx")
                    })?;
                    match tx {
                        TxVariant::V0(ref var) => match var {
                            TxVariantV0::OwnerTx(_) => Box::new(tx),
                            _ => {
                                return Err(Error::new(
                                    io::ErrorKind::InvalidData,
                                    "expected owner tx",
                                ))
                            }
                        },
                    }
                };
                let network_fee = cursor.take_asset()?;
                let token_supply = cursor.take_asset()?;
                Ok(Self::GetProperties(Properties {
                    height,
                    owner,
                    network_fee,
                    token_supply,
                }))
            }
            t if t == BodyType::GetBlock as u8 => {
                let filtered_type = cursor.take_u8()?;
                match filtered_type {
                    0 => {
                        let header = BlockHeader::deserialize(cursor)
                            .ok_or_else(|| Error::from(io::ErrorKind::UnexpectedEof))?;
                        let signer = cursor.take_sig_pair()?;
                        Ok(Self::GetBlock(FilteredBlock::Header((header, signer))))
                    }
                    1 => {
                        let block = Block::deserialize(cursor)
                            .ok_or_else(|| Error::from(io::ErrorKind::UnexpectedEof))?;
                        Ok(Self::GetBlock(FilteredBlock::Block(Arc::new(block))))
                    }
                    _ => Err(Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid GetBlock response",
                    )),
                }
            }
            t if t == BodyType::GetFullBlock as u8 => {
                let block = Block::deserialize(cursor)
                    .ok_or_else(|| Error::from(io::ErrorKind::UnexpectedEof))?;
                Ok(Self::GetFullBlock(Arc::new(block)))
            }
            t if t == BodyType::GetBlockRange as u8 => Ok(Self::GetBlockRange),
            t if t == BodyType::GetAddressInfo as u8 => {
                let net_fee = cursor.take_asset()?;
                let addr_fee = cursor.take_asset()?;
                let balance = cursor.take_asset()?;
                Ok(Self::GetAddressInfo(AddressInfo {
                    net_fee,
                    addr_fee,
                    balance,
                }))
            }
            _ => Err(Error::new(
                io::ErrorKind::InvalidData,
                "invalid msg response",
            )),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ErrorKind {
    Io,
    BytesRemaining,
    InvalidRequest,
    InvalidHeight,
    TxValidation(TxErr),
}

impl ErrorKind {
    fn serialize(self, buf: &mut Vec<u8>) {
        match self {
            Self::Io => buf.push(0x00),
            Self::BytesRemaining => buf.push(0x01),
            Self::InvalidRequest => buf.push(0x02),
            Self::InvalidHeight => buf.push(0x03),
            Self::TxValidation(err) => {
                buf.push(0x04);
                err.serialize(buf);
            }
        }
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0x00 => Self::Io,
            0x01 => Self::BytesRemaining,
            0x02 => Self::InvalidRequest,
            0x03 => Self::InvalidHeight,
            0x04 => Self::TxValidation(TxErr::deserialize(cursor)?),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize ErrorKind",
                ))
            }
        })
    }
}
