use super::*;
use clap::ArgMatches;
use godcoin::{constants::*, prelude::*};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

#[macro_use]
pub mod util;
pub mod account;

use util::{send_print_rpc_req, send_rpc_req};

pub fn create_wallet(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let state = wallet.db.state();
    if state != DbState::New {
        if state == DbState::Locked {
            println!("Wallet is locked, use unlock to use the existing wallet");
            return Ok(());
        } else if state == DbState::Unlocked {
            println!("Existing wallet already unlocked");
            return Ok(());
        } else {
            return Err(format!("Unknown state: {:?}", state));
        }
    }

    let pass = args.value_of("password").unwrap();
    wallet.db.set_password(pass.as_bytes());
    wallet.prompt = "locked>> ".to_owned();
    Ok(())
}

pub fn unlock(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let state = wallet.db.state();
    if state != DbState::Locked {
        if state == DbState::New {
            println!("A wallet has not yet been created, use new to create one");
            return Ok(());
        } else if state == DbState::Unlocked {
            println!("Wallet already unlocked");
            return Ok(());
        }
        return Err(format!("Unknown state: {:?}", state));
    }

    let pass = args.value_of("password").unwrap();
    if wallet.db.unlock(pass.as_bytes()) {
        wallet.prompt = "unlocked>> ".to_owned();
    } else {
        println!("Failed to unlock wallet...incorrect password");
    }
    Ok(())
}

pub fn build_script(_wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let ops: Vec<&str> = args.values_of("ops").unwrap().collect();
    let script = script_builder::build(&ops);
    match script {
        Ok(script) => {
            if script.len() > MAX_SCRIPT_BYTE_SIZE {
                println!(
                    "WARNING: Script exceeds the max byte size {}",
                    MAX_SCRIPT_BYTE_SIZE
                );
            }
            println!("{:?}", script);
            println!("P2SH address => {}", ScriptHash::from(script).to_wif());
        }
        Err(e) => {
            println!("{:?}", e);
        }
    }
    Ok(())
}

pub fn check_script_size(_wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let hex = args.value_of("hex").unwrap();
    let script = Script::new(hex_to_bytes!(hex)?);
    if script.len() > MAX_SCRIPT_BYTE_SIZE {
        println!(
            "WARNING: Script exceeds the max byte size {}",
            MAX_SCRIPT_BYTE_SIZE
        );
    }
    let word = if script.len() == 1 { "byte" } else { "bytes" };
    println!("Script is {} {}", script.len(), word);
    Ok(())
}

pub fn script_to_p2sh(_wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let hex = args.value_of("hex").unwrap();
    let hash: ScriptHash = Script::new(hex_to_bytes!(hex)?).into();
    println!("P2SH address => {}", hash.to_wif());

    Ok(())
}

pub fn decode_tx(_wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let hex = args.value_of("hex").unwrap();
    let tx_bytes = hex_to_bytes!(hex)?;
    let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
    let tx = TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?;
    println!("{:#?}", tx);

    Ok(())
}

pub fn sign_tx(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);
    let hex = args.value_of("hex").unwrap();
    let accounts: Vec<&str> = args.values_of("account").unwrap().collect();

    let mut tx_bytes = hex_to_bytes!(hex)?;
    let mut tx = {
        let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
        TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?
    };

    for account in accounts {
        let account = wallet
            .db
            .get_account(account)
            .ok_or("Account does not exist")?;
        tx.append_sign(&account);
    }

    tx_bytes.clear();
    tx_bytes.reserve(128);
    tx.serialize(&mut tx_bytes);
    println!("{}", faster_hex::hex_string(&tx_bytes).unwrap());

    Ok(())
}

pub fn unsign_tx(_wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let sig_pos: usize = args
        .value_of("index")
        .unwrap()
        .parse()
        .map_err(|_| "Failed to parse signature position".to_owned())?;

    let mut tx_bytes = hex_to_bytes!(args.value_of("hex").unwrap())?;
    let mut tx = {
        let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
        TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?
    };

    if sig_pos < tx.sigs().len() {
        tx.sigs_mut().remove(sig_pos);
    }

    tx_bytes.clear();
    tx.serialize(&mut tx_bytes);
    println!("{}", faster_hex::hex_string(&tx_bytes).unwrap());

    Ok(())
}

pub fn broadcast(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let hex = args.value_of("hex").unwrap();
    let tx_bytes = hex_to_bytes!(hex)?;
    let tx = {
        let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
        TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?
    };

    send_print_rpc_req(wallet, rpc::Request::Broadcast(tx));
    Ok(())
}

pub fn build_mint_tx(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let nonce: u32 = {
        let mut nonce = [0; 4];
        sodiumoxide::randombytes::randombytes_into(&mut nonce);
        u32::from_ne_bytes(nonce)
    };

    let expiry: u64 = {
        let expiry: u64 = args
            .value_of("expiry")
            .unwrap()
            .parse()
            .map_err(|_| "Failed to parse expiry ms".to_owned())?;
        godcoin::get_epoch_time() + expiry
    };

    let amount = args
        .value_of("amount")
        .unwrap()
        .parse()
        .map_err(|_| "Failed to parse asset")?;
    let script: Script = hex_to_bytes!(args.value_of("owner_script").unwrap())?.into();

    let res = send_rpc_req(wallet, rpc::Request::GetProperties)?;
    let owner = match res.body {
        Body::Response(rpc::Response::GetProperties(props)) => props.owner,
        _ => return Err("Failed to get blockchain properties".to_owned()),
    };
    let owner_wallet = match owner.as_ref() {
        TxVariant::V0(owner) => match owner {
            TxVariantV0::OwnerTx(owner) => &owner.wallet,
            _ => unreachable!("blockchain properties must be an owner tx"),
        },
    };

    let (attachment, attachment_name) =
        if let Some(attachment_path) = args.value_of("attachment_path") {
            let path = Path::new(attachment_path);
            let mut file = File::open(path).map_err(|e| {
                let cur_dir = std::env::current_dir().unwrap();
                format!("Failed to open file: {:?} (cwd: {:?})", e, cur_dir)
            })?;
            let meta = file
                .metadata()
                .map_err(|e| format!("Failed to query file metadata: {:?}", e))?;
            let mut buf = Vec::with_capacity(meta.len() as usize);
            file.read_to_end(&mut buf)
                .map_err(|e| format!("Failed to read file entirely: {:?}", e))?;
            (buf, args.value_of("attachment_name").unwrap())
        } else {
            (vec![], "")
        };

    let mint_tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: Tx {
            nonce,
            expiry,
            fee: Asset::new(0),
            signature_pairs: vec![],
        },
        to: owner_wallet.clone(),
        amount,
        attachment,
        attachment_name: attachment_name.to_owned(),
        script,
    }));
    let mut buf = Vec::with_capacity(4096);
    mint_tx.serialize(&mut buf);
    println!("{}", faster_hex::hex_string(&buf).unwrap());

    Ok(())
}

pub fn build_transfer_tx(_wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let nonce: u32 = {
        let mut nonce = [0; 4];
        sodiumoxide::randombytes::randombytes_into(&mut nonce);
        u32::from_ne_bytes(nonce)
    };

    let expiry: u64 = {
        let expiry: u64 = args
            .value_of("expiry")
            .unwrap()
            .parse()
            .map_err(|_| "Failed to parse expiry ms".to_owned())?;
        godcoin::get_epoch_time() + expiry
    };

    let from_script = Script::new(hex_to_bytes!(args.value_of("from_script").unwrap())?);

    let call_fn = args
        .value_of("call_fn")
        .unwrap()
        .parse()
        .map_err(|e| format!("Failed to parse call_fn id: {}", e))?;
    let call_args = if let Some(args) = args.value_of("args") {
        hex_to_bytes!(args)?
    } else {
        vec![]
    };

    let amount = args
        .value_of("amount")
        .unwrap()
        .parse()
        .map_err(|_| "Failed to parse asset amount")?;
    let fee = args
        .value_of("fee")
        .unwrap()
        .parse()
        .map_err(|_| "Failed to parse asset fee")?;
    let memo = args.value_of("memo").unwrap_or("").as_bytes();

    let transfer_tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
        base: Tx {
            nonce,
            expiry,
            fee,
            signature_pairs: vec![],
        },
        from: ScriptHash::from(&from_script),
        script: from_script,
        call_fn,
        args: call_args,
        amount,
        memo: memo.into(),
    }));

    let mut buf = Vec::with_capacity(4096);
    transfer_tx.serialize(&mut buf);
    println!("{}", faster_hex::hex_string(&buf).unwrap());

    Ok(())
}

pub fn get_properties(wallet: &mut Wallet, _args: &ArgMatches) -> Result<(), String> {
    send_print_rpc_req(wallet, rpc::Request::GetProperties);
    Ok(())
}

pub fn get_block(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let height: u64 = args
        .value_of("height")
        .unwrap()
        .parse()
        .map_err(|_| "Failed to parse height argument".to_owned())?;

    send_print_rpc_req(wallet, rpc::Request::GetBlock(height));
    Ok(())
}
