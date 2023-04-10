use clap::Parser;
use std::collections::HashMap;
use std::io::BufReader;

use ics::{components::Property, Event, ICalendar};
use regex::Regex;
use url::Url;

#[derive(Parser, Debug)]
struct Args {
    source: Url,

    // Blacklist rules
    #[clap(short, long)]
    blacklist: Vec<String>,
}

fn main() {
    let args = Args::parse();

    let source = reqwest::blocking::get(args.source).unwrap();
    let source = source.text().unwrap();

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

    for calendar in reader {
        let calendar = calendar.unwrap();
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

        output_calendar.write(std::io::stdout()).unwrap();
    }
}
