use actix_web::HttpResponse;
use godcoin::net::ResponseType;

pub trait IntoHttpResponse {
    fn into_res(self) -> HttpResponse;
}

impl IntoHttpResponse for ResponseType {
    fn into_res(self) -> HttpResponse {
        let mut buf = Vec::new();
        self.serialize(&mut buf);
        HttpResponse::Ok().body(buf)
    }
}
