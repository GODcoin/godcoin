use actix_web::{middleware, server, App, HttpRequest, HttpResponse};
use env_logger::{Env, DEFAULT_FILTER_ENV};
use std::env;

fn index(_: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().body("Hello world")
}

fn main() {
    env_logger::init_from_env(Env::new().filter_or(DEFAULT_FILTER_ENV, "godcoin=info,actix=info"));
    godcoin::init().unwrap();

    let sys = actix::System::new("godcoin-server");

    server::HttpServer::new(|| {
        App::new()
            .middleware(middleware::Logger::new(r#"%a "%r" %s %T"#))
            .resource("/", |r| r.with(index))
            .default_resource(|r| {
                r.with(|_: HttpRequest| HttpResponse::NotFound().body("Not found"))
            })
    })
    .bind(env::var("GODCOIN_BIND_ADDR").unwrap_or("127.0.0.1:8080".to_owned()))
    .unwrap()
    .start();

    sys.run();
}
