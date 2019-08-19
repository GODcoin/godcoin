use crate::{
    prelude::{verify::TxErr, *},
    serializer::*,
};
use std::io::{self, Cursor, Error};

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub body: RequestBody,
}

impl Request {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        self.body.serialize(buf);
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        Ok(Self {
            body: RequestBody::deserialize(cursor)?
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub body: ResponseBody,
}

impl Response {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        self.body.serialize(buf);
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        Ok(Self {
            body: ResponseBody::deserialize(cursor)?
        })
    }
}

#[repr(u8)]
pub enum BodyType {
    // Returned to clients when an error occurred processing a request
    Error = 0x01,

    // Operations that can update the blockchain state
    Broadcast = 0x10,

    // Getters
    GetProperties = 0x20,
    GetBlock = 0x21,
    GetBlockHeader = 0x22,
    GetAddressInfo = 0x23,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RequestBody {
    Broadcast(TxVariant),
    GetProperties,
    GetBlock(u64),       // height
    GetBlockHeader(u64), // height
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
            Self::GetProperties => buf.push(BodyType::GetProperties as u8),
            Self::GetBlock(height) => {
                buf.reserve_exact(9);
                buf.push(BodyType::GetBlock as u8);
                buf.push_u64(*height);
            }
            Self::GetBlockHeader(height) => {
                buf.reserve_exact(9);
                buf.push(BodyType::GetBlockHeader as u8);
                buf.push_u64(*height);
            }
            Self::GetAddressInfo(addr) => {
                buf.reserve_exact(33);
                buf.push(BodyType::GetAddressInfo as u8);
                buf.push_script_hash(addr);
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
            t if t == BodyType::GetProperties as u8 => Ok(Self::GetProperties),
            t if t == BodyType::GetBlock as u8 => {
                let height = cursor.take_u64()?;
                Ok(Self::GetBlock(height))
            }
            t if t == BodyType::GetBlockHeader as u8 => {
                let height = cursor.take_u64()?;
                Ok(Self::GetBlockHeader(height))
            }
            t if t == BodyType::GetAddressInfo as u8 => {
                let addr = cursor.take_script_hash()?;
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
    GetProperties(Properties),
    GetBlock(Box<Block>),
    GetBlockHeader {
        header: BlockHeader,
        signer: SigPair,
    },
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
        use std::mem;

        match self {
            Self::Error(err) => {
                buf.reserve_exact(2048);
                buf.push(BodyType::Error as u8);
                err.serialize(buf);
            }
            Self::Broadcast => buf.push(BodyType::Broadcast as u8),
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
                block.serialize(buf);
            }
            Self::GetBlockHeader { header, signer } => {
                buf.reserve_exact(256);
                buf.push(BodyType::GetBlockHeader as u8);
                header.serialize(buf);
                buf.push_sig_pair(signer);
            }
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
                let block = Block::deserialize(cursor)
                    .ok_or_else(|| Error::from(io::ErrorKind::UnexpectedEof))?;
                Ok(Self::GetBlock(Box::new(block)))
            }
            t if t == BodyType::GetBlockHeader as u8 => {
                let header = BlockHeader::deserialize(cursor)
                    .ok_or_else(|| Error::from(io::ErrorKind::UnexpectedEof))?;
                let signer = cursor.take_sig_pair()?;
                Ok(Self::GetBlockHeader { header, signer })
            }
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
    InvalidHeight,
    TxValidation(TxErr),
}

impl ErrorKind {
    fn serialize(self, buf: &mut Vec<u8>) {
        match self {
            Self::Io => buf.push(0),
            Self::BytesRemaining => buf.push(1),
            Self::InvalidHeight => buf.push(2),
            Self::TxValidation(err) => {
                buf.push(3);
                err.serialize(buf);
            }
        }
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0 => Self::Io,
            1 => Self::BytesRemaining,
            2 => Self::InvalidHeight,
            3 => Self::TxValidation(TxErr::deserialize(cursor)?),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize ErrorKind",
                ))
            }
        })
    }
}
