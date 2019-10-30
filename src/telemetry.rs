use ::libhoney::Value;
use libhoney::FieldHolder;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct Telemetry(Mutex<libhoney::Client<libhoney::transmission::Transmission>>);

impl Telemetry {
    pub fn new(cfg: libhoney::Config) -> Self {
        let honeycomb_client = libhoney::init(cfg);

        // publishing requires &mut so just mutex-wrap it, lmao (FIXME)
        Telemetry(Mutex::new(honeycomb_client))
    }
}

impl Telemetry {
    pub fn report_data(&self, data: HashMap<String, Value>) {
        // succeed or die. failure is unrecoverable (mutex poisoned)
        let mut client = self.0.lock().unwrap();
        let mut ev = client.new_event();
        ev.add(data);
        let res = ev.send(&mut client);
        if let Err(err) = res {
            // unable to report telemetry so log msg to stderr
            eprintln!("error sending event to honeycomb, {:?}", err);
        }
    }
}
