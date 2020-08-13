use super::Serializable;
use bytes::{BufMut, BytesMut};
use godcoin::serializer::BufRead;
use std::io::{self, Cursor};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    RequestVote(RequestVoteReq),
    AppendEntries(AppendEntriesReq),
}

impl Serializable<Self> for Request {
    fn serialize(&self, dst: &mut BytesMut) {
        match self {
            Self::RequestVote(req) => {
                dst.put_u8(0x01);
                dst.put_u64(req.term);
            }
            Self::AppendEntries(req) => {
                dst.put_u8(0x02);
                dst.put_u64(req.term);
            }
        }
    }

    fn byte_size(&self) -> usize {
        let size = match self {
            Self::RequestVote(_) => 8,
            Self::AppendEntries(_) => 8,
        };
        // Add 1 byte for the tag type
        size + 1
    }

    fn deserialize(src: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = src.take_u8()?;
        match tag {
            0x01 => {
                let term = src.take_u64()?;
                Ok(Self::RequestVote(RequestVoteReq { term }))
            }
            0x02 => {
                let term = src.take_u64()?;
                Ok(Self::AppendEntries(AppendEntriesReq { term }))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid tag type on Request",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestVoteReq {
    /// Term of the candidate
    pub term: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendEntriesReq {
    /// Current term of the leader
    pub term: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    RequestVote(RequestVoteRes),
    AppendEntries(AppendEntriesRes),
}

impl Serializable<Self> for Response {
    fn serialize(&self, dst: &mut BytesMut) {
        match self {
            Self::RequestVote(req) => {
                dst.put_u8(0x01);
                dst.put_u64(req.current_term);
                dst.put_u8(req.approved.into());
            }
            Self::AppendEntries(req) => {
                dst.put_u8(0x02);
                dst.put_u64(req.current_term);
                dst.put_u8(req.success.into());
            }
        }
    }

    fn byte_size(&self) -> usize {
        let size = match self {
            Self::RequestVote(_) => 9,
            Self::AppendEntries(_) => 9,
        };
        // Add 1 byte for the tag type
        size + 1
    }

    fn deserialize(src: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = src.take_u8()?;
        match tag {
            0x01 => {
                let current_term = src.take_u64()?;
                let approved = src.take_u8()? != 0;
                Ok(Self::RequestVote(RequestVoteRes {
                    current_term,
                    approved,
                }))
            }
            0x02 => {
                let current_term = src.take_u64()?;
                let success = src.take_u8()? != 0;
                Ok(Self::AppendEntries(AppendEntriesRes {
                    current_term,
                    success,
                }))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid tag type on Request",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestVoteRes {
    /// Term of the current node
    pub current_term: u64,
    /// Whether the current node approves the other node becoming a leader
    pub approved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendEntriesRes {
    /// Term of the current node
    pub current_term: u64,
    /// Whether the entries were successfully committed
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_request_vote_req() {
        test_req_serialization(Request::RequestVote(RequestVoteReq { term: 1234 }));
    }

    #[test]
    fn serialize_append_entries_req() {
        test_req_serialization(Request::AppendEntries(AppendEntriesReq { term: 1234 }));
    }

    #[test]
    fn serialize_request_vote_res() {
        test_res_serialization(Response::RequestVote(RequestVoteRes {
            current_term: 1234,
            approved: true,
        }));

        test_res_serialization(Response::RequestVote(RequestVoteRes {
            current_term: 1234,
            approved: false,
        }));
    }

    #[test]
    fn serialize_append_entries_res() {
        test_res_serialization(Response::AppendEntries(AppendEntriesRes {
            current_term: 1234,
            success: true,
        }));

        test_res_serialization(Response::AppendEntries(AppendEntriesRes {
            current_term: 1234,
            success: false,
        }));
    }

    fn test_req_serialization(req_a: Request) {
        let mut bytes = BytesMut::with_capacity(req_a.byte_size());
        req_a.serialize(&mut bytes);
        verify_byte_len(&bytes, req_a.byte_size());

        let req_b = Request::deserialize(&mut Cursor::new(bytes.as_ref())).unwrap();
        assert_eq!(req_a, req_b);
    }

    fn test_res_serialization(res_a: Response) {
        let mut bytes = BytesMut::with_capacity(res_a.byte_size());
        res_a.serialize(&mut bytes);
        verify_byte_len(&bytes, res_a.byte_size());

        let res_b = Response::deserialize(&mut Cursor::new(bytes.as_ref())).unwrap();
        assert_eq!(res_a, res_b);
    }

    fn verify_byte_len(bytes: &BytesMut, expected_size: usize) {
        assert_eq!(bytes.len(), expected_size);
        assert_eq!(bytes.capacity(), expected_size);
    }
}
