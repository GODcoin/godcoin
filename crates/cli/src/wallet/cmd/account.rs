use super::*;
use clap::ArgMatches;

pub fn create(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);
    let account_name = args.value_of("name").unwrap();
    if wallet.db.get_account(account_name).is_some() {
        println!("Account already exists");
        return Ok(());
    }
    let key = KeyPair::gen();
    wallet.db.set_account(account_name, &key.1);
    println!("Public key => {}", key.0.to_wif());
    println!("Private key => {}", key.1.to_wif());
    Ok(())
}

pub fn import(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);
    let account_name = args.value_of("name").unwrap();
    let wif = PrivateKey::from_wif(args.value_of("wif").unwrap())
        .map_err(|_| "Invalid wif".to_owned())?;
    for (acc, pair) in wallet.db.get_accounts() {
        if &acc == account_name {
            println!("Account already exists");
            return Ok(());
        } else if pair.1 == wif.1 {
            println!("Wif already exists under account `{}`", &acc);
            return Ok(());
        }
    }
    wallet.db.set_account(account_name, &wif.1);
    Ok(())
}

pub fn get(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);
    let account_name = args.value_of("name").unwrap();
    let key = wallet.db.get_account(account_name);
    match key {
        Some(key) => {
            println!("Public key => {}", key.0.to_wif());
            println!("Private key => {}", key.1.to_wif());
            println!("P2SH address => {}", ScriptHash::from(key.0).to_wif());
        }
        None => {
            println!("Account not found");
        }
    }
    Ok(())
}

pub fn get_addr_info(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);

    let address_name = args.value_of("address").unwrap();
    let script_hash = match wallet.db.get_account(address_name) {
        Some(key) => ScriptHash::from(key.0),
        None => ScriptHash::from_wif(address_name)
            .map_err(|e| format!("Invalid account or key: {:?}", e))?,
    };

    send_print_rpc_req(wallet, rpc::Request::GetAddressInfo(script_hash));
    Ok(())
}

pub fn delete(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);
    let account_name = args.value_of("name").unwrap();
    if wallet.db.del_account(account_name) {
        println!("Account permanently deleted");
    } else {
        println!("Account not found");
    }
    Ok(())
}

pub fn list(wallet: &mut Wallet, _args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);
    println!("Accounts:");
    for (acc, key) in wallet.db.get_accounts() {
        println!("  {} => {}", acc, key.0.to_wif());
    }
    Ok(())
}
