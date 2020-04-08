use super::*;
use clap::ArgMatches;
use db::WalletAccount;
use godcoin::tx::CreateAccountTx;

pub fn account_id_to_address(_wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let id = args
        .value_of("id")
        .unwrap()
        .parse()
        .map_err(|_| "Failed to parse decimal ID")?;
    let addr = AccountId::to_wif(&id);
    println!("Address: {}", addr);
    Ok(())
}

pub fn build_create_tx(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);

    let nonce = {
        let mut bytes = [0; 4];
        sodiumoxide::randombytes::randombytes_into(&mut bytes);
        u32::from_ne_bytes(bytes)
    };

    let creator = args.value_of("creator").unwrap();
    let creator = match wallet.db.get_account(creator) {
        Some(acc) => acc.id,
        None => AccountId::from_wif(creator)
            .map_err(|e| format!("Failed to parse account address: {:?}", e))?,
    };

    let expiry = {
        let expiry: u64 = args
            .value_of("expiry")
            .unwrap()
            .parse()
            .map_err(|_| "Failed to parse expiry ms".to_string())?;
        godcoin::get_epoch_time() + expiry
    };

    let fee = args
        .value_of("fee")
        .unwrap()
        .parse()
        .map_err(|_| "Failed to parse asset for the fee")?;

    let account = {
        let id = {
            let mut bytes = [0; 8];
            sodiumoxide::randombytes::randombytes_into(&mut bytes);
            AccountId::from_ne_bytes(bytes)
        };

        let balance = args
            .value_of("balance")
            .unwrap()
            .parse()
            .map_err(|_| "Failed to parse asset for the balance")?;

        let permissions = {
            let threshold = args
                .value_of("threshold")
                .unwrap()
                .parse()
                .map_err(|_| "Failed to parse threshold integer")?;
            let keys = {
                let vals: Vec<&str> = args.values_of("public_wif").unwrap().collect();
                let mut keys = vec![];
                for v in vals {
                    let key = PublicKey::from_wif(v)
                        .map_err(|_| format!("Failed to parse wif: {}", v))?;
                    keys.push(key);
                }
                keys
            };
            Permissions { threshold, keys }
        };
        let mut account = Account::create_default(id, permissions);
        account.balance = balance;

        if let Some(script) = args.value_of("script") {
            account.script = Script::new(hex_to_bytes!(script)?);
        }

        account
    };

    let tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
        base: Tx {
            nonce,
            expiry,
            fee,
            signature_pairs: vec![],
        },
        creator,
        account,
    }));

    let mut buf = Vec::with_capacity(8192);
    tx.serialize(&mut buf);
    println!("{}", faster_hex::hex_string(&buf).unwrap());

    Ok(())
}

pub fn build_update_tx(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    let nonce = {
        let mut bytes = [0; 4];
        sodiumoxide::randombytes::randombytes_into(&mut bytes);
        u32::from_ne_bytes(bytes)
    };

    let expiry = {
        let expiry: u64 = args
            .value_of("expiry")
            .unwrap()
            .parse()
            .map_err(|_| "Failed to parse expiry ms".to_string())?;
        godcoin::get_epoch_time() + expiry
    };

    let fee = args
        .value_of("fee")
        .unwrap()
        .parse()
        .map_err(|_| "Failed to parse asset for the fee")?;

    let account_id = args.value_of("account").unwrap();
    let account_id = match wallet.db.get_account(account_id) {
        Some(acc) => acc.id,
        None => AccountId::from_wif(account_id)
            .map_err(|e| format!("Failed to parse account address: {:?}", e))?,
    };

    let new_script = match args.value_of("script") {
        Some(hex) => Some(Script::new(hex_to_bytes!(hex)?)),
        None => None,
    };

    let new_permissions = match args.value_of("threshold") {
        Some(threshold) => {
            let threshold = threshold
                .parse()
                .map_err(|_| "Failed to parse threshold integer")?;
            let keys = {
                let vals: Vec<&str> = args.values_of("public_wif").unwrap().collect();
                let mut keys = vec![];
                for v in vals {
                    let key = PublicKey::from_wif(v)
                        .map_err(|_| format!("Failed to parse wif: {}", v))?;
                    keys.push(key);
                }
                keys
            };
            let perms = Permissions { threshold, keys };
            if !perms.is_valid() {
                return Err("Permissions threshold or key count is incorrect".to_string());
            }
            Some(perms)
        }
        None => None,
    };

    let tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
        base: Tx {
            nonce,
            expiry,
            fee,
            signature_pairs: vec![],
        },
        account_id,
        new_script,
        new_permissions,
    }));

    let mut buf = Vec::with_capacity(8192);
    tx.serialize(&mut buf);
    println!("{}", faster_hex::hex_string(&buf).unwrap());

    Ok(())
}

pub fn import(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);
    let acc_name = args.value_of("name").unwrap();
    let acc_address = args.value_of("account").unwrap();
    let acc_address = AccountId::from_wif(acc_address)
        .map_err(|_| format!("Failed to parse account address: {}", acc_address))?;
    let keys = {
        let mut keys = vec![];
        let wifs: Vec<&str> = args.values_of("wif").unwrap().collect();
        for wif in wifs {
            let wif = PrivateKey::from_wif(wif).map_err(|_| format!("Invalid wif: {}", wif))?;
            keys.push(wif);
        }
        keys
    };

    for (name, acc) in wallet.db.get_accounts() {
        if name == acc_name {
            println!("An account with this name already exists");
            return Ok(());
        } else if acc.id == acc_address {
            println!(
                "Account address already exists under the account name `{}`",
                name
            );
            return Ok(());
        }
    }

    let wallet_acc = WalletAccount {
        id: acc_address,
        keys,
    };

    wallet.db.set_account(acc_name, wallet_acc);
    Ok(())
}

pub fn get(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);
    let account_name = args.value_of("name").unwrap();
    let acc = wallet.db.get_account(account_name);
    match acc {
        Some(acc) => {
            println!("Account ID => {}", acc.id);
            println!("Account address => {}", acc.id.to_wif());
            for (index, key) in acc.keys.iter().enumerate() {
                println!("Key #{}", index);
                println!("  Public key => {}", key.0.to_wif());
                println!("  Private key => {}", key.1.to_wif());
            }
        }
        None => {
            println!("Account not found");
        }
    }
    Ok(())
}

pub fn get_acc_info(wallet: &mut Wallet, args: &ArgMatches) -> Result<(), String> {
    check_unlocked!(wallet);

    let account_id = args.value_of("account").unwrap();
    let account_id = match wallet.db.get_account(account_id) {
        Some(acc) => acc.id,
        None => AccountId::from_wif(account_id)
            .map_err(|e| format!("Invalid account or key: {:?}", e))?,
    };

    send_print_rpc_req(wallet, rpc::Request::GetAccountInfo(account_id));
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
    for (name, acc) in wallet.db.get_accounts() {
        println!("  {} => {} (raw ID: {})", name, acc.id.to_wif(), acc.id);
    }
    Ok(())
}
