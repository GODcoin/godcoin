use actix_web::HttpResponse;
use godcoin::net::MsgResponse;

pub trait IntoHttpResponse {
    fn into_res(self) -> HttpResponse;
}

impl IntoHttpResponse for MsgResponse {
    fn into_res(self) -> HttpResponse {
        let buf = self.serialize();
        HttpResponse::Ok().body(buf)
    }
}
