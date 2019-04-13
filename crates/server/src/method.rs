use actix_web::HttpResponse;
use serde_json::Value;
use crate::jsonrpc::*;

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

pub fn process_req(req: Request) -> HttpResponse {
    if req.jsonrpc != "2.0" {
        let msg = "must be a jsonrpc 2.0 request".to_owned();
        let data = Value::String("server only supports jsonrpc \"2.0\" specification".to_owned());
        let info = ErrorInfo::with_data(ErrCode::InvalidReq, msg, data);
        return ErrResponse::new(req.id, info).into()
    }

    let val = match req.method.as_ref() {
        "hello_world" => create_res(&req, Value::String("Hello world!".to_owned())),
        _ => create_err_res(&req, ErrCode::MethodNotFound.into()).into()
    };
    if let Some(val) = val {
        let json = serde_json::to_string(&val).expect("failed to convert value to string");
        HttpResponse::Ok().body(json).into()
    } else {
        HttpResponse::Ok().into()
    }
}
