// #![deny(warnings)]
// #![deny(missing_docs)]

//! # arpav
//!
//! Arpav client

use std::env;

use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use hyper::client::connect::Connect;
use hyper::service::{make_service_fn, service_fn};

use futures_util::TryStreamExt;

use chrono::offset::Utc;

use serde::Deserialize;

use serde_json::{json, value::Value};

#[derive(Deserialize)]
struct Contenitore {
    #[serde(rename = "FORNITORE")]
    pub fornitore: String,
    #[serde(rename = "ISTANTERUN")]
    pub istanterun: u64,
    #[serde(rename = "NOTE")]
    pub note: String,
    #[serde(rename = "LICENZA")]
    pub licenza: String,
    #[serde(rename = "PERIODO")]
    pub periodo: String,
    #[serde(rename = "INIZIO")]
    pub inizio: u64,
    #[serde(rename = "FINE")]
    pub fine: u64,
    #[serde(rename = "PROJECTION")]
    pub projection: String,
    #[serde(rename = "STAZIONE")]
    pub stazione: Stazione,
}

#[derive(Deserialize)]
struct Stazione {
    #[serde(rename = "IDSTAZ")]
    pub idstaz: u16,
    #[serde(rename = "NOME")]
    pub nome: String,
    #[serde(rename = "X")]
    pub x: f64,
    #[serde(rename = "Y")]
    pub y: f64,
    #[serde(rename = "QUOTA")]
    pub quota: u8,
    #[serde(rename = "TIPOSTAZ")]
    pub tipostaz: String,
    #[serde(rename = "PROVINCIA")]
    pub provincia: String,
    #[serde(rename = "COMUNE")]
    pub comune: String,
    #[serde(rename = "ATTIVAZIONE")]
    pub attivazione: String,
    #[serde(rename = "SENSORE")]
    pub sensore: Vec<Sensore>,
}

#[derive(Deserialize)]
struct Sensore {
    #[serde(rename = "ID")]
    pub id: u64,
    #[serde(rename = "PARAMNM")]
    pub paramnm: String,
    #[serde(rename = "TYPE")]
    pub _type: String,
    #[serde(rename = "UNITNM")]
    pub unitnm: String,
    #[serde(rename = "UNITCODE")]
    pub unitcode: u8,
    #[serde(rename = "NOTE")]
    pub note: String,
    #[serde(rename = "FREQ")]
    pub freq: u8,
    #[serde(rename = "DATI")]
    pub dati: Vec<Dati>,
}

#[derive(Debug, Deserialize)]
struct Dati {
    #[serde(rename = "ISTANTE")]
    istante: u64,
    #[serde(rename = "VM")]
    vm: f64,
}

async fn call<C>(client: &Client<C>, mut hour: i8) -> Result<Contenitore, String>
where C: Connect + 'static {
    while hour >= 0 {
        let url = format!("http://www.arpa.veneto.it/bollettini/meteo/h24/img{:02}/0182.xml", hour).parse().map_err(|e| format!("error parsing URL: {}", e))?;
        let res = client.get(url).await.map_err(|e| format!("error getting response: {}", e))?;
        if res.status().is_success() {
            let chunks = res.into_body().try_concat().await.map_err(|e| format!("error concatting Body: {}", e))?.to_vec();
            let body = String::from_utf8_lossy(chunks.as_slice());
            return serde_xml_rs::from_reader(body.as_bytes()).map_err(|e| format!("error parsing XML: {}", e));
        }
        else {
            hour -= 1;
        }
    }

    Err(String::from("no measurements found for today"))
}

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn dispatcher(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match req.method() {
        &Method::GET => {
            let client = Client::new();
            let now = Utc::now();
            let hour = now.format("%H").to_string();
            match call(&client, hour.parse().expect("error parsing Hour")).await {
                Ok(contenitore) => {
                    let mut v = json!({});
                    for sensore in &contenitore.stazione.sensore {
                        match sensore.dati.iter().last() {
                            Some(dati) => v[&sensore._type] = Value::from(dati.vm),
                            None => {},
                        }
                    }
                    Ok(Response::new(Body::from(v.to_string())))
                },
                Err(e) => {
                    let mut error = Response::new(Body::from(e));
                    *error.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    Ok(error)
                }
            }
        },
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        },
    }
}

fn get_port() -> u16 {
    env::var("PORT")
        .map_err(|_| ())
        .and_then(|s| s.parse().map_err(|_| ()))
        .unwrap_or_else(|_| 8080)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = make_service_fn(|_| {
        async {
            Ok::<_, hyper::Error>(service_fn(dispatcher))
        }
    });

    let addr = ([0, 0, 0, 0], get_port()).into();
    let server = Server::bind(&addr)
        .serve(service);

    server.await?;

    Ok(())
}
