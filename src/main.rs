use std::{collections::HashMap, io::BufWriter};
use std::fs::File;
use std::io::BufReader;

use ics::{Event, ICalendar, components::Property};
use regex::Regex;

fn main() {
    let buf = BufReader::new(File::open("/tmp/component.ics").unwrap());
    // hashMap of blacklists
    let mut blacklist = HashMap::<String, Vec<Regex>>::new();
    blacklist.insert(
        "SUMMARY".to_string(),
        vec![Regex::new(r"^.*TU.*$").unwrap()],
    );

    let reader = ical::IcalParser::new(buf);

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

            let mut output_event =
                Event::new(props.get("UID").unwrap().clone(), props.get("DTSTAMP").unwrap().clone());

            e.properties.iter().for_each(|p| {
                output_event.push(Property::new(p.name.clone(), p.value.clone().unwrap()));
            });

            output_calendar.add_event(output_event);
        });

        output_calendar.write(std::io::stdout()).unwrap();

    }
}
