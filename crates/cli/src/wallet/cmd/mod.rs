use super::*;
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

pub fn create_wallet(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    let state = wallet.db.state();
    if state != DbState::New {
        if state == DbState::Locked {
            println!("Use unlock to use the existing wallet");
            return Ok(());
        } else if state == DbState::Unlocked {
            println!("Existing wallet already unlocked");
            return Ok(());
        } else {
            return Err(format!("Unknown state: {:?}", state));
        }
    }

    check_args!(args, 1);
    let pass = &args[1].as_ref();
    wallet.db.set_password(pass);
    wallet.prompt = "locked>> ".to_owned();
    Ok(())
}

pub fn unlock(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
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

    check_args!(args, 1);
    let pass = &args[1].as_ref();
    if wallet.db.unlock(pass) {
        wallet.prompt = "unlocked>> ".to_owned();
    } else {
        println!("Failed to unlock wallet...incorrect password");
    }
    Ok(())
}

pub fn build_script(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    let script = script_builder::build(&args[1..]);
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

pub fn check_script_size(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_args!(args, 1);
    let script = Script::new(hex_to_bytes!(args[1])?);
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

pub fn script_to_p2sh(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_args!(args, 1);
    let hash: ScriptHash = Script::new(hex_to_bytes!(args[1])?).into();
    println!("P2SH address => {}", hash.to_wif());

    Ok(())
}

pub fn decode_tx(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_args!(args, 1);

    let tx_bytes = hex_to_bytes!(args[1])?;
    let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
    let tx = TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?;
    println!("{:#?}", tx);

    Ok(())
}

pub fn sign_tx(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_unlocked!(wallet);
    check_at_least_args!(args, 2);

    let mut tx_bytes = hex_to_bytes!(args[1])?;
    let mut tx = {
        let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
        TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?
    };

    for account in &args[2..] {
        let account = wallet
            .db
            .get_account(account)
            .ok_or("Account does not exist")?;
        match &tx {
            TxVariant::V0(var) => {
                if let TxVariantV0::RewardTx(_) = var {
                    return Err("Cannot sign reward tx".to_owned());
                }
            }
        }
        tx.append_sign(&account);
    }

    tx_bytes.clear();
    tx_bytes.reserve(128);
    tx.serialize(&mut tx_bytes);
    println!("{}", faster_hex::hex_string(&tx_bytes).unwrap());

    Ok(())
}

pub fn unsign_tx(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_args!(args, 2);
    let sig_pos: usize = args[1]
        .parse()
        .map_err(|_| "Failed to parse signature position".to_owned())?;

    let mut tx_bytes = hex_to_bytes!(args[2])?;
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

pub fn broadcast(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_args!(args, 1);
    let tx_bytes = hex_to_bytes!(args[1])?;
    let tx = {
        let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
        TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?
    };

    send_print_rpc_req(wallet, rpc::Request::Broadcast(tx));
    Ok(())
}

pub fn build_mint_tx(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_args!(args, 4);
    let expiry: u64 = {
        let expiry: u64 = args[1]
            .parse()
            .map_err(|_| "Failed to parse expiry ms".to_owned())?;
        godcoin::get_epoch_ms() + expiry
    };

    let amount = args[2].parse().map_err(|_| "Failed to parse grael asset")?;
    let script: Script = hex_to_bytes!(args[3])?.into();

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

    let (attachment, attachment_name) = if !args[4].is_empty() {
        let path = Path::new(&args[4]);
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

        let file_name = path.file_name().unwrap().to_str().unwrap();
        (buf, file_name.to_owned())
    } else {
        (vec![], "".to_owned())
    };

    let mint_tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: Tx {
            expiry,
            signature_pairs: vec![],
            fee: Asset::new(0),
        },
        to: owner_wallet.clone(),
        amount,
        attachment,
        attachment_name,
        script,
    }));
    let mut buf = Vec::with_capacity(4096);
    mint_tx.serialize(&mut buf);
    println!("{}", faster_hex::hex_string(&buf).unwrap());

    Ok(())
}

pub fn build_transfer_tx(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_args!(args, 6);

    let expiry: u64 = {
        let expiry: u64 = args[1]
            .parse()
            .map_err(|_| "Failed to parse expiry ms".to_owned())?;
        godcoin::get_epoch_ms() + expiry
    };

    let from_script = Script::new(hex_to_bytes!(args[2])?);
    let to_script = ScriptHash::from_wif(&args[3])
        .map_err(|e| format!("Failed to parse P2SH address: {}", e))?;

    let amount = args[4]
        .parse()
        .map_err(|_| "Failed to parse grael asset amount")?;
    let fee = args[5]
        .parse()
        .map_err(|_| "Failed to parse grael asset fee")?;
    let memo = args[6].as_bytes();

    let transfer_tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
        base: Tx {
            expiry,
            signature_pairs: vec![],
            fee,
        },
        from: ScriptHash::from(&from_script),
        to: to_script,
        script: from_script,
        amount,
        memo: memo.into(),
    }));

    let mut buf = Vec::with_capacity(4096);
    transfer_tx.serialize(&mut buf);
    println!("{}", faster_hex::hex_string(&buf).unwrap());

    Ok(())
}

pub fn get_properties(wallet: &mut Wallet, _args: &mut Vec<String>) -> Result<(), String> {
    send_print_rpc_req(wallet, rpc::Request::GetProperties);
    Ok(())
}

pub fn get_block(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_args!(args, 1);
    let height: u64 = args[1]
        .parse()
        .map_err(|_| "Failed to parse height argument".to_owned())?;

    send_print_rpc_req(wallet, rpc::Request::GetBlock(height));
    Ok(())
}
