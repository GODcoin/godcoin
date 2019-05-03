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

macro_rules! send_rpc_req {
    ($wallet:expr, $req:expr) => {{
        let res = Client::new()
            .post($wallet.url.clone())
            .body($req.serialize())
            .send();
        match res {
            Ok(mut res) => {
                let len = res.content_length().unwrap_or(0);
                let mut content = Vec::with_capacity(len as usize);
                res.read_to_end(&mut content)
                    .map_err(|e| format!("{}", e))?;
                let mut cursor = Cursor::<&[u8]>::new(&content);
                MsgResponse::deserialize(&mut cursor)
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
        let mut dst = Vec::with_capacity(len);
        dst.resize(len, 0);
        let res = faster_hex::hex_decode($string.as_bytes(), &mut dst);
        match res {
            Ok(_) => Ok(dst),
            Err(_) => Err("invalid hex string"),
        }
    }};
}
