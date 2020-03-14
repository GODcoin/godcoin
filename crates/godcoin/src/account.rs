use crate::{
    asset::Asset,
    crypto::{PublicKey, SigPair},
    script::{Arg, Builder, FnBuilder, OpFrame, Script},
    serializer::*,
};
use std::io::{self, Cursor};

pub type AccountId = u64;

pub const MAX_PERM_KEYS: u8 = 8;
pub const IMMUTABLE_ACCOUNT_THRESHOLD: u8 = 0xFF;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub balance: Asset,
    pub script: Script,
    pub permissions: Permissions,
    pub destroyed: bool,
}

impl Account {
    pub fn create_default(id: AccountId, perms: Permissions) -> Self {
        Self {
            id,
            balance: Asset::default(),
            script: Builder::new()
                .push(
                    FnBuilder::new(0x00, OpFrame::OpDefine(vec![Arg::AccountId, Arg::Asset]))
                        .push(OpFrame::AccountId(id))
                        .push(OpFrame::OpCheckPermsFastFail)
                        .push(OpFrame::OpTransfer)
                        .push(OpFrame::True),
                )
                .build()
                .unwrap(),
            permissions: perms,
            destroyed: false,
        }
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push_u64(self.id);
        buf.push_asset(self.balance);
        buf.push_bytes(&self.script);
        self.permissions.serialize(buf);
        buf.push(self.destroyed as u8);
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let id = cur.take_u64()?;
        let balance = cur.take_asset()?;
        let script = Script::new(cur.take_bytes()?);
        let permissions = Permissions::deserialize(cur)?;
        let destroyed = cur.take_u8()? != 0;
        Ok(Self {
            id,
            balance,
            script,
            permissions,
            destroyed,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permissions {
    pub threshold: u8,
    pub keys: Vec<PublicKey>,
}

impl Permissions {
    pub fn verify(&self, data: &[u8], sigs: &[SigPair]) -> Result<(), PermsSigVerifyErr> {
        if self.threshold == 0 {
            return Ok(());
        } else if usize::from(self.threshold) > sigs.len() {
            return Err(PermsSigVerifyErr::InsufficientThreshold);
        }

        let mut valid_threshold = 0;
        'sig_loop: for pair in sigs {
            for key in &self.keys {
                if key == &pair.pub_key {
                    if key.verify(data, &pair.signature) {
                        valid_threshold += 1;
                        continue 'sig_loop;
                    } else {
                        return Err(PermsSigVerifyErr::InvalidSig);
                    }
                }
            }
        }

        if valid_threshold > 0 {
            if valid_threshold >= self.threshold {
                Ok(())
            } else {
                Err(PermsSigVerifyErr::InsufficientThreshold)
            }
        } else {
            Err(PermsSigVerifyErr::NoMatchingSigs)
        }
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push(self.threshold);
        buf.push(self.keys.len() as u8);
        for key in &self.keys {
            buf.push_pub_key(key);
        }
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let threshold = cur.take_u8()?;
        let key_len = cur.take_u8()?;
        let mut keys = Vec::with_capacity(usize::from(key_len));
        for _ in 0..key_len {
            keys.push(cur.take_pub_key()?);
        }
        Ok(Self { threshold, keys })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermsSigVerifyErr {
    /// The given signatures did not meet the required threshold to succeed verification
    InsufficientThreshold,
    /// None of the signatures had valid public keys that could validate the data
    NoMatchingSigs,
    /// One of the signatures failed verification on the data
    InvalidSig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::{KeyPair, Signature},
        script::Script,
    };
    use sodiumoxide::crypto::sign;

    #[test]
    fn verify_equal_threshold() {
        let (account, keys) = create_dummy_account(4, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[0].sign(data));
        sigs.push(keys[1].sign(data));
        sigs.push(keys[2].sign(data));
        sigs.push(keys[3].sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));
    }

    #[test]
    fn verify_single_sig_threshold() {
        let (account, keys) = create_dummy_account(1, 1);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[0].sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));
    }

    #[test]
    fn verify_single_sig_fail_unment_threshold() {
        let (account, _) = create_dummy_account(1, 1);
        let data = "Hello world".as_bytes();
        assert_eq!(
            account.permissions.verify(data, &[]),
            Err(PermsSigVerifyErr::InsufficientThreshold)
        );
    }

    #[test]
    fn verify_last_two_sigs() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[2].sign(data));
        sigs.push(keys[3].sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));
    }

    #[test]
    fn verify_first_two_sigs() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[0].sign(data));
        sigs.push(keys[1].sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));
    }

    #[test]
    fn verify_sigs_with_gap_in_keys() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[0].sign(data));
        sigs.push(keys[3].sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));
    }

    #[test]
    fn verify_sigs_with_overqualified_threshold() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[0].sign(data));
        sigs.push(keys[1].sign(data));
        sigs.push(keys[2].sign(data));
        sigs.push(keys[3].sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));
    }

    #[test]
    fn verify_sigs_with_unknown_trailing_sig() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[0].sign(data));
        sigs.push(keys[1].sign(data));
        sigs.push(KeyPair::gen().sign(data));
        sigs.push(KeyPair::gen().sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));
    }

    #[test]
    fn verify_sigs_in_reverse_order() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[1].sign(data));
        sigs.push(keys[0].sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));
    }

    #[test]
    fn verify_sigs_fail_by_unmet_threshold() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[0].sign(data));

        assert_eq!(
            account.permissions.verify(data, &sigs),
            Err(PermsSigVerifyErr::InsufficientThreshold)
        );
    }

    #[test]
    fn verify_sigs_fail_with_invalid_sig() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(keys[0].sign(data));
        sigs.push(keys[1].sign(data));
        sigs.push(keys[2].sign(data));
        sigs.push(keys[3].sign(data));

        assert_eq!(account.permissions.verify(data, &sigs), Ok(()));

        sigs[3].signature = Signature(sign::Signature([0u8; sign::SIGNATUREBYTES]));
        assert_eq!(
            account.permissions.verify(data, &sigs),
            Err(PermsSigVerifyErr::InvalidSig)
        );
    }

    #[test]
    fn verify_sigs_fail_with_none_matching() {
        let (account, keys) = create_dummy_account(2, 4);
        let mut sigs = Vec::new();
        let data = "Hello world".as_bytes();
        sigs.push(KeyPair::gen().sign(data));
        sigs.push(KeyPair::gen().sign(data));

        assert_eq!(
            account.permissions.verify(data, &sigs),
            Err(PermsSigVerifyErr::NoMatchingSigs)
        );
    }

    fn create_dummy_account(threshold: u8, key_count: u8) -> (Account, Vec<KeyPair>) {
        let keys: Vec<KeyPair> = (0..key_count).map(|_| KeyPair::gen()).collect();
        let account = Account {
            id: 0,
            balance: Asset::default(),
            script: Script::new(vec![]),
            permissions: Permissions {
                threshold,
                keys: keys.iter().map(|kp| kp.0.clone()).collect(),
            },
            destroyed: false,
        };
        (account, keys)
    }
}
