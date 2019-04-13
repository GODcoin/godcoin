use crate::jsonrpc::*;
use serde_json::Value;

pub fn create_res(req: &Request, result: Value) -> Option<Value> {
    if let Some(id) = req.id {
        let res = serde_json::to_value(SuccessResponse::new(id, result));
        Some(res.expect("failed to serialize response"))
    } else {
        None
    }
}

pub fn create_err_res(req: &Request, error: ErrorInfo) -> Option<Value> {
    if req.id.is_some() {
        let res = serde_json::to_value(ErrResponse::new(req.id, error));
        Some(res.expect("failed to serialize response"))
    } else {
        None
    }
}

pub fn process_req(req: Request) -> Option<Value> {
    if req.jsonrpc != "2.0" {
        let msg = "must be a jsonrpc 2.0 request".to_owned();
        let data = Value::String("server only supports jsonrpc \"2.0\" specification".to_owned());
        let info = ErrorInfo::with_data(ErrCode::InvalidReq, msg, data);
        return create_err_res(&req, info);
    }

    match req.method.as_ref() {
        "hello_world" => create_res(&req, Value::String("Hello world!".to_owned())),
        _ => create_err_res(&req, ErrCode::MethodNotFound.into()).into(),
    }
}

pub fn process_req_value(value: Value) -> Option<Value> {
    let request = serde_json::from_value::<Request>(value);
    match request {
        Ok(request) => process_req(request),
        Err(e) => {
            use serde_json::error::Category;
            let code = match e.classify() {
                Category::Data => ErrCode::InvalidReq,
                _ => ErrCode::InternalError,
            };
            let info = ErrorInfo::new(code, e.to_string());
            Some(
                serde_json::to_value(ErrResponse::new(None, info))
                    .expect("failed to serialize response"),
            )
        }
    }
}
