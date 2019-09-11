// Fetches network stats from the Fios Quantum G1000 router
//
// Their admin interface fetches data using a standard JSON REST API. If there is a way to
// authenticate that is different than how a browser does, I don't know of it. So that's what I do
// here. And then I just call /api/network/1, parse the results, and send the data to influx db.
//
// The program expects the router to be found on myfiosgateway.com
//
// The authentication works this way:
// 1) You call /login to get a passwordSalt.
// 2) you take the Sha512(password + passwordSalt) to create a hash
// 3) you call /login with {"password": hash}
// 4) on successful login, two cookies are returned XSRF-TOKEN and Session
// For all API calls set Session as a cookie and a header X-XSRF-TOKEN with the XSRF-TOKEN value


// TODO
// * add tests
// * add Travis, https://docs.travis-ci.com/user/languages/rust/
// * nicer error handling. At least for: 1) unexpected data in /network/1 return, and 2) auth errors

#[macro_use]
extern crate simple_error;

use clap::{App, Arg};
use env_logger::{Env};
use log::debug;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Sha512, Digest};
use std::collections::HashMap;
use tokio;

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

#[derive(Debug, Default)]
struct AuthInfo {
    token: String,
    session: u32,
}

const BASE_URI:&str = "https://myfiosgateway.com/api/";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = App::new("Fios Gateway Stats Retriever")
        .version("0.1.0")
        .author("Allan Beaufour <allan@beaufour.dk>")
        .arg(Arg::with_name("password")
             .short("p")
             .long("password")
             .value_name("PASSWORD")
             .help("Password for router")
             .required(true)
             .takes_value(true))
        .arg(Arg::with_name("influx_db")
             .short("i")
             .long("influxdb")
             .value_name("URI")
             .help("URI to InfluxDB including databasename")
             .takes_value(true))
        .get_matches();
    let password = args.value_of("password").unwrap();

    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "info")
        .write_style_or("MY_LOG_STYLE", "always");
    env_logger::init_from_env(env);

    let client = reqwest::Client::builder()
        // Unknown CA, and I'm not sure all devices use the same...
        .danger_accept_invalid_certs(true)
        .build()?;

    let login_info = get_login_info(&client)?;
    debug!("Got login info: {:#?}", login_info);

    let auth_info = do_login(&client, &password, &login_info.passwordSalt)?;
    debug!("Got auth info: {:#?}", auth_info);

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::HeaderName::from_static("x-xsrf-token"),
                   reqwest::header::HeaderValue::from_str(&auth_info.token)?);
    // There is a cookie store on reqwest, but to set the default x-xsrf-token header I need to
    // create a new client...
    headers.insert(reqwest::header::HeaderName::from_static("cookie"),
                   reqwest::header::HeaderValue::from_str(&format!("Session={};", auth_info.session))?);

    let authed_client = reqwest::Client::builder()
        // Unknown CA, and I'm not sure all devices use the same...
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .build()?;

    let raw_data = fetch_api(&authed_client, "network/1")?;
    let data:Value = serde_json::from_str(&raw_data)?;
    debug!("Got network response: {:#?}", data);

    let rx = data["bandwidth"]["minutesRx"][0].as_u64().unwrap() * 8;
    let tx = data["bandwidth"]["minutesTx"][0].as_u64().unwrap() * 8;
    let errors = data["rxErrors"].as_u64().unwrap() * 8;
    let dropped = data["rxDropped"].as_u64().unwrap() * 8;
    println!("Data: rx = {}, tx = {}, errors = {}, dropped = {}", rx, tx, errors, dropped);

    // TODO: there's also natEntriesUsed from /api/settings/system, which might be interesting to pull

    if let Some(influx_db) = args.value_of("influx_db") {
        let mut influx_map = HashMap::new();
        influx_map.insert("net_tx", tx);
        influx_map.insert("net_rx", rx);
        influx_map.insert("net_rx_errors", errors);
        influx_map.insert("net_rx_dropped", dropped);
        let influx_data = influx_map.iter().fold(String::new(), |mut acc, (key, val)| {
            acc.push_str(&format!("{},host=myfiosgateway.com value={}i\n", key, val));
            acc
        });
        debug!("Influx data:\n{}", influx_data);
        save_data(&client, influx_db, influx_data)?;
    }

    fetch_api(&authed_client, "logout")?;

    Ok(())
}

fn get_login_info(client: &reqwest::Client) -> Result<LoginResponse, FetchError>
{
    let body = fetch_api(client, "login")?;
    let info = serde_json::from_str(&body)?;
    Ok(info)
}

fn fetch_api(client: &reqwest::Client, api: &str) -> Result<String, FetchError> {
    let uri = reqwest::Url::parse(&format!("{}{}", BASE_URI, api))?;
    debug!("Fetching: {}", uri);
    let mut response = client.get(uri).send()?;
    let body = response.text()?;
    Ok(body)
}

fn do_login(client: &reqwest::Client, password: &str, password_salt: &str) -> Result<AuthInfo, FetchError> {
    let mut info = AuthInfo::default();

    let mut hasher = Sha512::new();
    hasher.input(password);
    hasher.input(password_salt);
    let hash = hasher.result();
    let json = format!("{{\"password\":\"{:x}\"}}", hash);

    let uri = reqwest::Url::parse(&format!("{}login", BASE_URI))?;
    let response = client.post(uri)
        .header(reqwest::header::CONTENT_TYPE, "application/json;charset=UTF-8")
        .body(json)
        .send()?;

    if response.status().is_success() {
        for cookie in response.cookies() {
            match cookie.name() {
                "XSRF-TOKEN" => info.token = cookie.value().to_string(),
                "Session" => info.session = cookie.value().parse().unwrap(),
                _ => () ,
            }
        }
    } else {
        bail!("Could not login: {}", response.status());
    }

    Ok(info)
}

fn save_data(client: &reqwest::Client, influx_uri:&str, data: String) -> Result<(), FetchError> {
    debug!("Saving data to InfluxDB: {}", influx_uri);
    let response = client.post(influx_uri)
        .header(reqwest::header::CONTENT_TYPE, "application/json;charset=UTF-8")
        .body(data)
        .send()?;

    if response.status() != reqwest::StatusCode::NO_CONTENT {
        bail!("Unexpected status from InfluxDB: {}", response.status());
    }
    Ok(())
}

#[derive(Debug)]
enum FetchError {
    Http(reqwest::Error),
    Url(reqwest::UrlError),
    Json(serde_json::Error),
    Simple(simple_error::SimpleError),
}

impl From<reqwest::Error> for FetchError {
    fn from(err: reqwest::Error) -> FetchError {
        FetchError::Http(err)
    }
}

impl From<reqwest::UrlError> for FetchError {
    fn from(err: reqwest::UrlError) -> FetchError {
        FetchError::Url(err)
    }
}

impl From<serde_json::Error> for FetchError {
    fn from(err: serde_json::Error) -> FetchError {
        FetchError::Json(err)
    }
}

impl From<simple_error::SimpleError> for FetchError {
    fn from(err: simple_error::SimpleError) -> FetchError {
        FetchError::Simple(err)
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "fetch error")
    }
}

impl std::error::Error for FetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
