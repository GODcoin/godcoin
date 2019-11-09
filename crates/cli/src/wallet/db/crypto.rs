use sodiumoxide::crypto::secretbox;

pub fn encrypt_with_key(msg: &[u8], key: &secretbox::Key) -> Vec<u8> {
    let nonce = secretbox::gen_nonce();
    let cipher_text = secretbox::seal(msg, &nonce, key);

    let mut enc = Vec::with_capacity(secretbox::NONCEBYTES + cipher_text.len());
    enc.extend_from_slice(nonce.as_ref());
    enc.extend_from_slice(cipher_text.as_ref());
    enc
}

pub fn decrypt_with_key(msg: &[u8], key: &secretbox::Key) -> Option<Vec<u8>> {
    let nonce = &msg[0..secretbox::NONCEBYTES];
    let nonce = secretbox::Nonce::from_slice(nonce).unwrap();
    let msg = &msg[secretbox::NONCEBYTES..];
    secretbox::open(msg, &nonce, &key).ok()
}
