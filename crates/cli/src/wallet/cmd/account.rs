use super::*;

pub fn create(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_unlocked!(wallet);
    check_args!(args, 1);
    let account = &args[1];
    if wallet.db.get_account(account).is_some() {
        println!("Account already exists");
        return Ok(());
    }
    let key = KeyPair::gen();
    wallet.db.set_account(account, &key.1);
    println!("Public key => {}", key.0.to_wif());
    println!("Private key => {}", key.1.to_wif());
    Ok(())
}

pub fn import(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_unlocked!(wallet);
    check_args!(args, 2);
    let account = &args[1];
    let wif = PrivateKey::from_wif(&args[2]).map_err(|_| "Invalid wif".to_owned())?;
    for (acc, pair) in wallet.db.get_accounts() {
        if &acc == account {
            println!("Account already exists");
            return Ok(());
        } else if pair.1 == wif.1 {
            println!("Wif already exists under account `{}`", &acc);
            return Ok(());
        }
    }
    wallet.db.set_account(account, &wif.1);
    Ok(())
}

pub fn get(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_unlocked!(wallet);
    check_args!(args, 1);
    let key = wallet.db.get_account(&args[1]);
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

pub fn get_addr_info(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_unlocked!(wallet);
    check_args!(args, 1);

    let script_hash = match wallet.db.get_account(&args[1]) {
        Some(key) => ScriptHash::from(key.0),
        None => ScriptHash::from_wif(&args[1])
            .map_err(|e| format!("Invalid account or key: {:?}", e))?,
    };

    send_print_rpc_req(wallet, rpc::Request::GetAddressInfo(script_hash));
    Ok(())
}

pub fn delete(wallet: &mut Wallet, args: &mut Vec<String>) -> Result<(), String> {
    check_unlocked!(wallet);
    check_args!(args, 1);
    if wallet.db.del_account(&args[1]) {
        println!("Account permanently deleted");
    } else {
        println!("Account not found");
    }
    Ok(())
}

pub fn list(wallet: &mut Wallet, _args: &mut Vec<String>) -> Result<(), String> {
    check_unlocked!(wallet);
    println!("Accounts:");
    for (acc, key) in wallet.db.get_accounts() {
        println!("  {} => {}", acc, key.0.to_wif());
    }
    Ok(())
}
