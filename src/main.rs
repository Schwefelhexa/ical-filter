use clap::Parser;
use std::net::SocketAddr;

use http_body_util::Full;
use hyper::body::Bytes;

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use ics::{components::Property, Event, ICalendar};
use regex::Regex;
use std::collections::HashMap;
use std::convert::Infallible;
use std::io::BufReader;
use tokio::net::TcpListener;
use url::Url;

#[derive(Parser, Debug)]
struct Args {
    source: Url,

    // Blacklist rules
    #[clap(short, long)]
    blacklist: Vec<String>,
}

async fn filter_ical(
    _: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let args = Args::parse();

    let source = reqwest::get(args.source).await.unwrap();
    let source = source.text().await.unwrap();

    let reader = ical::IcalParser::new(BufReader::new(source.as_bytes()));

    let mut blacklist = HashMap::<String, Vec<Regex>>::new();
    args.blacklist.iter().for_each(|b| {
        let mut split = b.splitn(2, '=');
        let key = split.next().unwrap();
        let value = split.next().unwrap();

        let value = Regex::new(value).unwrap();

        if let Some(v) = blacklist.get_mut(key) {
            v.push(value);
        } else {
            blacklist.insert(key.to_string(), vec![value]);
        }
    });

    let calendar = reader.into_iter().next().unwrap().unwrap();
    let events = calendar.events.iter().filter(|e| {
        e.properties.iter().all(|p| {
            if let Some(blacklist) = blacklist.get(&p.name) {
                blacklist
                    .iter()
                    .all(|r| !r.is_match(&p.value.clone().unwrap()))
            } else {
                true
            }
        })
    });
    let events = events.collect::<Vec<_>>();

    let cal_version = calendar
        .properties
        .iter()
        .find(|p| p.name == "VERSION")
        .unwrap()
        .value
        .clone()
        .unwrap();
    let prod_id = calendar
        .properties
        .iter()
        .find(|p| p.name == "PRODID")
        .unwrap()
        .value
        .clone()
        .unwrap();

    let mut output_calendar = ICalendar::new(cal_version, prod_id);

    events.iter().for_each(|e| {
        let props: HashMap<String, String> = e
            .properties
            .iter()
            .map(|p| (p.name.clone(), p.value.clone().unwrap()))
            .collect();

        let mut output_event = Event::new(
            props.get("UID").unwrap().clone(),
            props.get("DTSTAMP").unwrap().clone(),
        );

        e.properties.iter().for_each(|p| {
            output_event.push(Property::new(p.name.clone(), p.value.clone().unwrap()));
        });

        output_calendar.add_event(output_event);
    });

    let mut buf = Vec::new();
    output_calendar.write(&mut buf).unwrap();

    let mut res = Response::new(Full::from(buf));
    res.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("text/calendar;charset=UTF-8"),
    );
    res.headers_mut().insert(
        hyper::header::CONTENT_DISPOSITION,
        hyper::header::HeaderValue::from_static("attachment; filename=calendar.ics"),
    );
    Ok(res)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = ([0, 0, 0, 0], 3000).into();

    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(stream, service_fn(filter_ical))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}
