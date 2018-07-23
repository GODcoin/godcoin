macro_rules! tx_deref {
    ($ty:ty) => {
        impl ::std::ops::Deref for $ty {
            type Target = Tx;

            fn deref(&self) -> &Tx {
                &self.base
            }
        }
    };
}
