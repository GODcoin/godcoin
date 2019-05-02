use super::{db::Password, *};
use godcoin::prelude::*;
use reqwest::Client;
use std::{
    io::{Cursor, Read},
    time::{SystemTime, UNIX_EPOCH},
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
            println!("{:?}", script);
            println!("{:?}", ScriptHash::from(script));
        }
        Err(e) => {
            println!("{:?}", e);
        }
    }
    Ok(true)
}

pub fn build_mint_tx(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<bool, String> {
    check_args!(args, 4);
    let timestamp: u64 = {
        let ts: u64 = args[1]
            .parse()
            .map_err(|_| "Failed to parse timestamp offset".to_owned())?;
        ts + SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    };

    let balance = {
        let gold: Asset = args[2].parse().map_err(|_| "Failed to parse gold asset")?;
        let silver: Asset = args[3]
            .parse()
            .map_err(|_| "Failed to parse silver asset")?;
        if gold.symbol != AssetSymbol::GOLD {
            return Err("Expected gold asset".to_owned());
        } else if silver.symbol != AssetSymbol::SILVER {
            return Err("Expected silver asset".to_owned());
        }
        Balance::from(gold, silver).unwrap()
    };

    let script: Script = {
        let src = &args[4];
        let len = src.len() / 2;
        let mut dst = Vec::with_capacity(len);
        dst.resize(len, 0);
        faster_hex::hex_decode(src.as_bytes(), &mut dst).map_err(|_| "invalid hex string")?;
        dst.into()
    };

    let res = send_rpc_req!(wallet, MsgRequest::GetProperties)?;
    let owner = match res {
        MsgResponse::GetProperties(props) => props,
        _ => return Err("wallet not unlocked".to_owned()),
    }
        .owner;

    let mint_tx = TxVariant::MintTx(MintTx {
        base: Tx {
            tx_type: TxType::MINT,
            timestamp,
            signature_pairs: vec![],
            fee: "0 GOLD".parse().unwrap(),
        },
        to: owner.wallet,
        balance,
        script,
    });
    let mut buf = Vec::with_capacity(4096);
    mint_tx.encode_with_sigs(&mut buf);
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
