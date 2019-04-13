use actix_web::HttpResponse;
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Option<u32>,
    pub method: String,
    pub params: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    jsonrpc: &'static str,
    id: u32,
    result: serde_json::Value,
}

impl SuccessResponse {
    pub fn new(id: u32, result: serde_json::Value) -> Self {
        SuccessResponse {
            jsonrpc: "2.0",
            id,
            result,
        }
    }
}

impl Into<HttpResponse> for SuccessResponse {
    fn into(self) -> HttpResponse {
        let json = serde_json::to_string(&self).expect("failed to serialize ErrResponse");
        HttpResponse::Ok().body(json)
    }
}

#[derive(Debug, Serialize)]
pub struct ErrResponse {
    jsonrpc: &'static str,
    id: Option<u32>,
    error: ErrorInfo,
}

impl ErrResponse {
    pub fn new(id: Option<u32>, error: ErrorInfo) -> Self {
        ErrResponse {
            jsonrpc: "2.0",
            id,
            error,
        }
    }
}

impl Into<HttpResponse> for ErrResponse {
    fn into(self) -> HttpResponse {
        let json = serde_json::to_string(&self).expect("failed to serialize ErrResponse");
        HttpResponse::Ok().body(json)
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorInfo {
    pub code: ErrCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ErrorInfo {
    pub fn new(code: ErrCode, message: String) -> Self {
        ErrorInfo {
            code,
            message,
            data: None,
        }
    }

    pub fn with_data(code: ErrCode, message: String, data: serde_json::Value) -> Self {
        ErrorInfo {
            code,
            message,
            data: Some(data),
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(i32)]
#[allow(dead_code)]
pub enum ErrCode {
    ParseError = -32700,
    InvalidReq = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
}

impl Into<ErrorInfo> for ErrCode {
    fn into(self) -> ErrorInfo {
        let message = match self {
            ErrCode::ParseError => "error parsing json",
            ErrCode::InvalidReq => "invalid request",
            ErrCode::MethodNotFound => "method not found",
            ErrCode::InvalidParams => "parameters unacceptable for method",
            ErrCode::InternalError => "an unknown error has occurred",
        }
        .to_owned();
        ErrorInfo {
            code: self,
            message,
            data: None,
        }
    }
}

impl Serialize for ErrCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(*self as i32)
    }
}
