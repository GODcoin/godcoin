use super::{db::Password, *};
use godcoin::{constants::*, prelude::*};
use reqwest::Client;
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

#[macro_use]
pub mod util;

pub mod account;

pub fn create_wallet(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    let state = wallet.db.state();
    if state != DbState::New {
        if state == DbState::Locked {
            println!("Use unlock to use the existing wallet");
            return Ok(false);
        } else if state == DbState::Unlocked {
            println!("Existing wallet already unlocked");
            return Ok(false);
        } else {
            return Err(format!("Unknown state: {:?}", state));
        }
    }

    check_args!(args, 1);
    let pass = &Password(args.remove(1).into_bytes());
    wallet.db.set_password(pass);
    wallet.prompt = "locked>> ".to_owned();
    Ok(false)
}

pub fn unlock(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    let state = wallet.db.state();
    if state != DbState::Locked {
        if state == DbState::New {
            println!("A wallet has not yet been created, use new to create one");
            return Ok(false);
        } else if state == DbState::Unlocked {
            println!("Wallet already unlocked");
            return Ok(false);
        }
        return Err(format!("Unknown state: {:?}", state));
    }

    check_args!(args, 1);
    let pass = &Password(args.remove(1).into_bytes());
    if wallet.db.unlock(pass) {
        wallet.prompt = "unlocked>> ".to_owned();
    } else {
        println!("Failed to unlock wallet...incorrect password");
    }
    Ok(false)
}

pub fn build_script(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
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
    Ok(true)
}

pub fn check_script_size(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
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
    Ok(true)
}

pub fn script_to_p2sh(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    check_args!(args, 1);
    let hash: ScriptHash = Script::new(hex_to_bytes!(args[1])?).into();
    println!("P2SH address => {}", hash.to_wif());

    Ok(true)
}

pub fn decode_tx(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    check_args!(args, 1);

    let tx_bytes = hex_to_bytes!(args[1])?;
    let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
    let tx = TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?;
    println!("{:#?}", tx);

    Ok(true)
}

pub fn sign_tx(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
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
        match &mut tx {
            TxVariant::OwnerTx(tx) => tx.append_sign(&account),
            TxVariant::MintTx(tx) => tx.append_sign(&account),
            TxVariant::RewardTx(_) => return Err("Cannot sign reward tx".to_owned()),
            TxVariant::TransferTx(tx) => tx.append_sign(&account),
        }
    }

    tx_bytes.clear();
    tx_bytes.reserve(128);
    tx.serialize(&mut tx_bytes);
    println!("{}", faster_hex::hex_string(&tx_bytes).unwrap());

    Ok(true)
}

pub fn unsign_tx(_wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    check_args!(args, 2);
    let sig_pos: usize = args[1]
        .parse()
        .map_err(|_| "Failed to parse signature position".to_owned())?;

    let mut tx_bytes = hex_to_bytes!(args[2])?;
    let mut tx = {
        let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
        TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?
    };

    if sig_pos < tx.signature_pairs.len() {
        tx.signature_pairs.remove(sig_pos);
    }

    tx_bytes.clear();
    tx.serialize(&mut tx_bytes);
    println!("{}", faster_hex::hex_string(&tx_bytes).unwrap());

    Ok(true)
}

pub fn broadcast(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    check_args!(args, 1);
    let tx_bytes = hex_to_bytes!(args[1])?;
    let tx = {
        let cursor = &mut Cursor::<&[u8]>::new(&tx_bytes);
        TxVariant::deserialize(cursor).ok_or("Failed to decode tx")?
    };

    send_print_rpc_req!(wallet, net::MsgRequest::Broadcast(tx));
    Ok(true)
}

pub fn build_mint_tx(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    check_args!(args, 4);
    let timestamp: u64 = {
        let ts: u64 = args[1]
            .parse()
            .map_err(|_| "Failed to parse timestamp offset".to_owned())?;
        ts + godcoin::util::get_epoch_ms()
    };

    let amount = args[2].parse().map_err(|_| "Failed to parse grael asset")?;
    let script: Script = hex_to_bytes!(args[3])?.into();

    let res = send_rpc_req!(wallet, MsgRequest::GetProperties)?;
    let owner = match res {
        MsgResponse::GetProperties(props) => props,
        _ => return Err("wallet not unlocked".to_owned()),
    }
    .owner;

    let (attachment, attachment_name) = if !args[4].is_empty() {
        let path = Path::new(&args[4]);
        let mut file = File::open(path).map_err(|e| format!("Failed to open file: {:?}", e))?;
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

    let mint_tx = TxVariant::MintTx(MintTx {
        base: Tx {
            tx_type: TxType::MINT,
            timestamp,
            signature_pairs: vec![],
            fee: Asset::new(0),
        },
        to: owner.wallet,
        amount,
        attachment,
        attachment_name,
        script,
    });
    let mut buf = Vec::with_capacity(4096);
    mint_tx.serialize(&mut buf);
    println!("{}", faster_hex::hex_string(&buf).unwrap());

    Ok(true)
}

pub fn get_properties(wallet: &mut Wallet, _args: &mut Vec<String>) -> Result<bool, String> {
    send_print_rpc_req!(wallet, MsgRequest::GetProperties);
    Ok(true)
}

pub fn get_block(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    check_args!(args, 1);
    let height: u64 = args[1]
        .parse()
        .map_err(|_| "Failed to parse height argument".to_owned())?;

    send_print_rpc_req!(wallet, MsgRequest::GetBlock(height));
    Ok(true)
}
