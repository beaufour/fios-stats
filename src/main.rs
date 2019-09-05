extern crate hyper;
extern crate native_tls;
extern crate hyper_tls;

use hyper::Client;
use hyper::client::{HttpConnector};
use hyper::rt::{self, Future, Stream};
use hyper_tls::HttpsConnector;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
struct LoginResponse {
    doSetupWizard: bool,
    requirePassword: bool,
    passwordSalt: String,
    isWireless: bool,
    error: u8,
    maxUsers: u8,
    denyState: u8,
    denyTimeout: u8,
    meshNetworkEnabledStatus: bool,
    meshUserEnabledConfig: bool,
}

fn main() {
    let uri = "https://myfiosgateway.com/api/login".parse().unwrap();

    let fut = fetch_json(uri)
        .map(|response| {
            println!("response: {:#?}", response);
            println!("salt: {}", response.passwordSalt);
        })
        .map_err(|e| {
            match e {
                FetchError::Http(e) => eprintln!("http error: {}", e),
                FetchError::Json(e) => eprintln!("json parsing error: {}", e),
            }
        });

    rt::run(fut);
}

fn fetch_json(uri: hyper::Uri) -> impl Future<Item=LoginResponse, Error=FetchError> {
    let mut tls_builder = native_tls::TlsConnector::builder();
    tls_builder.danger_accept_invalid_certs(true);
    let tls = tls_builder.build().unwrap();

    let mut http = HttpConnector::new(4);
    http.enforce_http(false);

    let https = HttpsConnector::from((http, tls));

    let client = Client::builder().build::<_, hyper::Body>(https);

    client
        .get(uri)
        .and_then(|res| {
            res.into_body().concat2()
        })
        .from_err::<FetchError>()
        .and_then(|body| {
            let response = serde_json::from_slice(&body)?;

            Ok(response)
        })
        .from_err()
}

enum FetchError {
    Http(hyper::Error),
    Json(serde_json::Error),
}

impl From<hyper::Error> for FetchError {
    fn from(err: hyper::Error) -> FetchError {
        FetchError::Http(err)
    }
}

impl From<serde_json::Error> for FetchError {
    fn from(err: serde_json::Error) -> FetchError {
        FetchError::Json(err)
    }
}
