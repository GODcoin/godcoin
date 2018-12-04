macro_rules! tx_deref {
    ($ty:ty) => {
        impl std::ops::Deref for $ty {
            type Target = Tx;

            #[inline(always)]
            fn deref(&self) -> &Tx {
                &self.base
            }
        }
    };
}

macro_rules! tx_sign {
    ($ty:ty) => {
        impl SignTx for $ty {
            #[inline]
            fn sign(&self, key_pair: &KeyPair) -> SigPair {
                let mut buf = Vec::with_capacity(4096);
                self.encode(&mut buf);
                key_pair.sign(&buf)
            }

            #[inline]
            fn append_sign(&mut self, key_pair: &KeyPair) {
                let pair = self.sign(key_pair);
                self.base.signature_pairs.push(pair);
            }

            fn verify_all(&self) -> bool {
                let mut buf = Vec::with_capacity(4096);
                self.encode(&mut buf);
                for pair in &self.base.signature_pairs {
                    if !pair.pub_key.verify(&buf, &pair.signature) {
                        return false
                    }
                }
                true
            }

            fn verify_keys(&self, keys: &[PublicKey]) -> bool {
                let mut iter_num = 0;
                let len = self.base.signature_pairs.len();

                let mut buf = Vec::with_capacity(4096);
                self.encode(&mut buf);

                for key in keys {
                    let mut found = false;
                    while iter_num < len {
                        let pair = &self.base.signature_pairs[iter_num];
                        if key == &pair.pub_key {
                            found = true;
                            if !pair.pub_key.verify(&buf, &pair.signature) {
                                return false
                            }
                            break;
                        }
                        iter_num += 1;
                    }
                    if !found {
                        return false
                    }
                }

                true
            }
        }
    }
}
