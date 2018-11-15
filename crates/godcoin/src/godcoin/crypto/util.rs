use sodiumoxide::crypto::hash::sha256;

#[inline]
pub fn double_sha256(buf: &[u8]) -> sha256::Digest {
    sha256::hash(sha256::hash(buf).as_ref())
}
