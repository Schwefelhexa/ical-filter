use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

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

        // filter events

        for ev in &events {
            println!(
                "{}",
                ev.properties
                    .iter()
                    .filter(|p| p.name == "SUMMARY")
                    .next()
                    .unwrap()
                    .value
                    .clone()
                    .unwrap()
            );
        }

        println!("\nCount: {}/{}", events.len(), calendar.events.len())
    }
}
