macro_rules! tx_deref {
    ($ty:ty) => {
        impl ::std::ops::Deref for $ty {
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
                let mut buf = Vec::new();
                self.encode(&mut buf);
                key_pair.sign(&buf)
            }

            #[inline]
            fn append_sign(&mut self, key_pair: &KeyPair) {
                let pair = self.sign(key_pair);
                self.base.signature_pairs.push(pair);
            }
        }
    }
}
