use clap::Parser;
use itertools::Itertools;
use std::net::SocketAddr;

use anyhow::{anyhow, Result};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use ics::{components::Property, Event, ICalendar};
use regex::Regex;
use std::collections::HashMap;
use std::io::BufReader;
use tokio::net::TcpListener;
use url::Url;

#[derive(Parser, Debug)]
struct Args {
    source: Url,

    // Blacklist rules
    #[clap(short, long)]
    blacklist: Vec<String>,

    // Dedup events with same start, end and name
    #[clap(short, long, default_value_t = false)]
    dedup: bool,
}

async fn filter_ical(_: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>> {
    let args = Args::parse();

    let source = reqwest::get(args.source).await?;
    let source = source.text().await?;

    let reader = ical::IcalParser::new(BufReader::new(source.as_bytes()));

    let mut blacklist = HashMap::<String, Vec<Regex>>::new();
    args.blacklist.iter().for_each(|b| {
        let (key, value) = match b.splitn(2, '=').collect_tuple() {
            Some((k, r)) => (k, r),
            None => {
                println!("Invalid blacklist rule: {}", b);
                return;
            }
        };

        let regex = match Regex::new(value) {
            Ok(regex) => regex,
            Err(e) => {
                println!("Invalid blacklist regex: {}\n{:?}", value, e);
                return;
            }
        };

        if let Some(v) = blacklist.get_mut(key) {
            v.push(regex);
        } else {
            blacklist.insert(key.to_string(), vec![regex]);
        }
    });

    let calendar = reader.into_iter().next();
    let calendar = calendar.ok_or(anyhow!("No calendar found"))??; // Unwrap calendar, returning anyhow error on failure
    let events = calendar.events.iter().filter(|e| {
        e.properties.iter().all(|p| {
            if let Some(blacklist) = blacklist.get(&p.name) {
                blacklist
                    .iter()
                    .all(|r| !r.is_match(&p.value.clone().unwrap_or("".to_string())))
            } else {
                true
            }
        })
    });
    let events = if args.dedup {
        println!("Deduping events");
        events
            .unique_by(|a| {
                let props: HashMap<_, _> = a
                    .properties
                    .iter()
                    .map(|p| (p.name.clone(), p.value.clone()))
                    .collect();

                (
                    props.get("DTSTART").cloned(),
                    props.get("DTEND").cloned(),
                    props.get("SUMMARY").cloned(),
                )
            })
            .collect::<Vec<_>>()
    } else {
        events.collect::<Vec<_>>()
    };

    let cal_props: HashMap<_, _> = calendar
        .properties
        .iter()
        .map(|p| (p.name.clone(), p.value.clone()))
        .collect();

    let cal_version = cal_props
        .get("VERSION")
        .cloned()
        .flatten()
        .ok_or(anyhow!("No VERSION found on calendar"))?;
    let prod_id = cal_props
        .get("PRODID")
        .cloned()
        .flatten()
        .ok_or(anyhow!("No PRODID found on calendar"))?;

    let mut output_calendar = ICalendar::new(cal_version, prod_id);

    let results: Vec<Result<_>> = events
        .iter()
        .map(|e| {
            let props: HashMap<String, String> = e
                .properties
                .iter()
                .map(|p| (p.name.clone(), p.value.clone().unwrap_or("".to_string())))
                .collect();

            let mut output_event = Event::new(
                props
                    .get("UID")
                    .cloned()
                    .ok_or(anyhow!("No UID found on event"))?,
                props
                    .get("DTSTAMP")
                    .cloned()
                    .ok_or(anyhow!("No DTSTAMP found on event"))?,
            );

            e.properties.iter().for_each(|p| {
                output_event.push(Property::new(
                    p.name.clone(),
                    p.value.clone().unwrap_or("".to_string()),
                ));
            });

            output_calendar.add_event(output_event);

            Ok(())
        })
        .collect();
    for result in results {
        if let Err(e) = result {
            println!("Error while filtering events: {:?}", e);
        }
    }

    let mut buf = Vec::new();
    output_calendar.write(&mut buf)?;

    let mut res = Response::new(Full::from(buf));
    res.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("text/calendar;charset=UTF-8"),
    );
    res.headers_mut().insert(
        hyper::header::CONTENT_DISPOSITION,
        hyper::header::HeaderValue::from_static("attachment; filename=calendar.ics"),
    );
    res.headers_mut().insert(
        hyper::header::VARY,
        hyper::header::HeaderValue::from_static("User-Agent"),
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
