macro_rules! check_unlocked {
    ($self:expr) => {
        if $self.db.state() != DbState::Unlocked {
            return Err("wallet not unlocked".to_owned());
        }
    };
}

macro_rules! check_args {
    ($args:expr, $count:expr) => {
        if $args.len() != $count + 1 {
            let word = if $count == 1 { "argument" } else { "arguments" };
            return Err(format!("Expected {} {}", $count, word));
        }
    };
}

macro_rules! check_at_least_args {
    ($args:expr, $count:expr) => {
        if $args.len() < $count + 1 {
            let word = if $count == 1 { "argument" } else { "arguments" };
            return Err(format!("Expected at least {} {}", $count, word));
        }
    };
}

macro_rules! send_rpc_req {
    ($wallet:expr, $req:expr) => {{
        let body = {
            let mut buf = Vec::with_capacity(4096);
            godcoin::net::RequestType::Single($req).serialize(&mut buf);
            buf
        };
        let res = Client::new().post($wallet.url.clone()).body(body).send();
        match res {
            Ok(mut res) => {
                let len = res.content_length().unwrap_or(0);
                let mut content = Vec::with_capacity(len as usize);
                res.read_to_end(&mut content)
                    .map_err(|e| format!("{}", e))?;
                let mut cursor = Cursor::<&[u8]>::new(&content);
                godcoin::net::ResponseType::deserialize(&mut cursor)
                    .map(|res| res.unwrap_single())
                    .map_err(|e| format!("Failed to deserialize response: {}", e))
            }
            Err(e) => Err(format!("{}", e)),
        }
    }};
}

macro_rules! send_print_rpc_req {
    ($wallet:expr, $req:expr) => {
        let res = send_rpc_req!($wallet, $req)?;
        println!("{:#?}", res);
    };
}

macro_rules! hex_to_bytes {
    ($string:expr) => {{
        let len = $string.len() / 2;
        let mut dst = vec![0; len];
        let res = faster_hex::hex_decode($string.as_bytes(), &mut dst);
        match res {
            Ok(_) => Ok(dst),
            Err(_) => Err("invalid hex string"),
        }
    }};
}
