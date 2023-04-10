use std::fs::File;
use std::io::BufReader;

fn main() {
    let buf = BufReader::new(File::open("/tmp/component.ics").unwrap());

    let reader = ical::IcalParser::new(buf);

    for calendar in reader {
        println!("{} events", calendar.unwrap().events.len());
    }
}

