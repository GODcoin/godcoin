use crate::{prelude::*, serializer::*};
use std::{
    io::{self, Cursor, Error},
    mem,
    sync::Arc,
};

#[repr(u8)]
pub enum RpcType {
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
pub enum Request {
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

impl Request {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Broadcast(tx) => {
                buf.reserve_exact(4096);
                buf.push(RpcType::Broadcast as u8);
                tx.serialize(buf);
            }
            Self::SetBlockFilter(filter) => {
                buf.reserve_exact(1 + (filter.len() * mem::size_of::<ScriptHash>()));
                buf.push(RpcType::SetBlockFilter as u8);
                buf.push(filter.len() as u8);
                for addr in filter {
                    buf.push_scripthash(&addr);
                }
            }
            Self::ClearBlockFilter => buf.push(RpcType::ClearBlockFilter as u8),
            Self::Subscribe => buf.push(RpcType::Subscribe as u8),
            Self::Unsubscribe => buf.push(RpcType::Unsubscribe as u8),
            Self::GetProperties => buf.push(RpcType::GetProperties as u8),
            Self::GetBlock(height) => {
                buf.reserve_exact(9);
                buf.push(RpcType::GetBlock as u8);
                buf.push_u64(*height);
            }
            Self::GetFullBlock(height) => {
                buf.reserve_exact(9);
                buf.push(RpcType::GetFullBlock as u8);
                buf.push_u64(*height);
            }
            Self::GetBlockRange(min_height, max_height) => {
                buf.reserve_exact(1 + (2 * mem::size_of::<u64>()));
                buf.push(RpcType::GetBlockRange as u8);
                buf.push_u64(*min_height);
                buf.push_u64(*max_height);
            }
            Self::GetAddressInfo(addr) => {
                buf.reserve_exact(33);
                buf.push(RpcType::GetAddressInfo as u8);
                buf.push_scripthash(&addr);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == RpcType::Broadcast as u8 => {
                let tx = TxVariant::deserialize(cursor)
                    .ok_or_else(|| Error::new(io::ErrorKind::InvalidData, "failed to decode tx"))?;
                Ok(Self::Broadcast(tx))
            }
            t if t == RpcType::SetBlockFilter as u8 => {
                let addr_len = usize::from(cursor.take_u8()?);
                let mut filter = BlockFilter::new();
                for _ in 0..addr_len {
                    filter.insert(ScriptHash(cursor.take_digest()?));
                }
                Ok(Self::SetBlockFilter(filter))
            }
            t if t == RpcType::ClearBlockFilter as u8 => Ok(Self::ClearBlockFilter),
            t if t == RpcType::Subscribe as u8 => Ok(Self::Subscribe),
            t if t == RpcType::Unsubscribe as u8 => Ok(Self::Unsubscribe),
            t if t == RpcType::GetProperties as u8 => Ok(Self::GetProperties),
            t if t == RpcType::GetBlock as u8 => {
                let height = cursor.take_u64()?;
                Ok(Self::GetBlock(height))
            }
            t if t == RpcType::GetFullBlock as u8 => {
                let height = cursor.take_u64()?;
                Ok(Self::GetFullBlock(height))
            }
            t if t == RpcType::GetBlockRange as u8 => {
                let min_height = cursor.take_u64()?;
                let max_height = cursor.take_u64()?;
                Ok(Self::GetBlockRange(min_height, max_height))
            }
            t if t == RpcType::GetAddressInfo as u8 => {
                let addr = ScriptHash(cursor.take_digest()?);
                Ok(Self::GetAddressInfo(addr))
            }
            _ => Err(Error::new(
                io::ErrorKind::InvalidData,
                "invalid rpc request",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Response {
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

impl Response {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Broadcast => buf.push(RpcType::Broadcast as u8),
            Self::SetBlockFilter => buf.push(RpcType::SetBlockFilter as u8),
            Self::ClearBlockFilter => buf.push(RpcType::ClearBlockFilter as u8),
            Self::Subscribe => buf.push(RpcType::Subscribe as u8),
            Self::Unsubscribe => buf.push(RpcType::Unsubscribe as u8),
            Self::GetProperties(props) => {
                buf.reserve_exact(4096 + mem::size_of::<Properties>());
                buf.push(RpcType::GetProperties as u8);
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
                buf.push(RpcType::GetBlock as u8);
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
                buf.push(RpcType::GetFullBlock as u8);
                block.serialize(buf);
            }
            Self::GetBlockRange => buf.push(RpcType::GetBlockRange as u8),
            Self::GetAddressInfo(info) => {
                buf.reserve_exact(1 + (mem::size_of::<Asset>() * 3));
                buf.push(RpcType::GetAddressInfo as u8);
                buf.push_asset(info.net_fee);
                buf.push_asset(info.addr_fee);
                buf.push_asset(info.balance);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == RpcType::Broadcast as u8 => Ok(Self::Broadcast),
            t if t == RpcType::SetBlockFilter as u8 => Ok(Self::SetBlockFilter),
            t if t == RpcType::ClearBlockFilter as u8 => Ok(Self::ClearBlockFilter),
            t if t == RpcType::Subscribe as u8 => Ok(Self::Subscribe),
            t if t == RpcType::Unsubscribe as u8 => Ok(Self::Unsubscribe),
            t if t == RpcType::GetProperties as u8 => {
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
            t if t == RpcType::GetBlock as u8 => {
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
            t if t == RpcType::GetFullBlock as u8 => {
                let block = Block::deserialize(cursor)
                    .ok_or_else(|| Error::from(io::ErrorKind::UnexpectedEof))?;
                Ok(Self::GetFullBlock(Arc::new(block)))
            }
            t if t == RpcType::GetBlockRange as u8 => Ok(Self::GetBlockRange),
            t if t == RpcType::GetAddressInfo as u8 => {
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
                "invalid rpc response",
            )),
        }
    }
}
