//! The companion's "Company" tab: the bus company the launcher has open, as a paired phone or
//! tablet sees it (`company::remote::summary`: money, today's dispositions, the depot and its
//! workshop), and the orders it may send for it.
//!
//! The company is the launcher's: the game only reads its file, and an order from a device is
//! checked against it (with the orders still waiting) and put in the company's queue, which
//! the launcher carries out with the same rules. The device names nothing but an order of a
//! fixed shape - no company, no file - and only a paired device gets here (`Route::needs_key`).
//!
//! While the player drives a duty of the company, the tab has the control room's messages to it
//! as the launcher writes them for his game (`messages::inbox_file`, in his language with the
//! buttons' words) - answered, and control told, by orders like the rest (`remote::Order::Reply`,
//! `remote::Order::Tell`).

use omsi_launcher_lib::company::{self as co, remote};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

/// Orders waiting for the launcher a device may add to.
const WAITING_MAX: usize = 20;
/// Today's plan is made again after this long (it reads the map's timetable).
const PLAN_FOR_SECS: u64 = 120;

/// Today's plan, for the company file it was made from.
struct Cached {
    key: (String, String, SystemTime),
    at: Instant,
    plan: Option<co::day::Plan>,
}

static PLAN: Mutex<Option<Cached>> = Mutex::new(None);

/// What the tab shows (`GET /api/company`).
pub(crate) fn state() -> Value {
    let data = omsi_launcher_lib::data_dir();
    let Some(c) = remote::latest(&data) else { return json!({ "none": true }) };
    let saved = std::fs::metadata(co::store::path_of(&data, &c.id)).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    let key = (c.id.clone(), c.date.clone(), saved);
    let plan = {
        let mut cache = PLAN.lock().unwrap_or_else(|e| e.into_inner());
        match cache.as_ref() {
            Some(x) if x.key == key && x.at.elapsed().as_secs() < PLAN_FOR_SECS => x.plan.clone(),
            _ => {
                let plan = remote::plan_of(&c);
                *cache = Some(Cached { key, at: Instant::now(), plan: plan.clone() });
                plan
            }
        }
    };
    let waiting = remote::pending(&data, &c.id);
    let mut v = remote::summary(&c, plan.as_ref(), &waiting);
    // (the driver's inbox, while he drives a duty of the company)
    if c.messages.driving.is_some() {
        let inbox = std::fs::read_to_string(co::messages::inbox_file(&data, &c.id)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
        v["messages"] = inbox.unwrap_or(Value::Null);
    }
    v
}

/// An order from the tab (`POST /api/company`): its status and answer. Refusals are the
/// rules' own words (English keys the page translates).
pub(crate) fn order(body: &[u8]) -> (u16, Value) {
    let Some(o) = serde_json::from_slice::<Value>(body).ok().as_ref().and_then(remote::parse) else { return (400, json!({ "error": "bad_order" })) };
    let data = omsi_launcher_lib::data_dir();
    let Some(c) = remote::latest(&data) else { return (404, json!({ "error": "No company is open in the launcher." })) };
    let waiting = remote::pending(&data, &c.id);
    if waiting.len() >= WAITING_MAX {
        return (429, json!({ "error": "Too many orders are waiting for the launcher." }));
    }
    if let Err(e) = remote::check(&c, &waiting, &o) {
        return (200, json!({ "error": e }));
    }
    match remote::queue(&data, &c.id, &o) {
        Ok(()) => {
            log::info!("companion: an order for the company {} queued: {o:?}", c.id);
            (200, json!({ "ok": true }))
        }
        Err(e) => {
            log::warn!("companion: cannot queue an order for {}: {e:#}", c.id);
            (500, json!({ "error": "The order could not be saved." }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::http::{self, Route};

    #[test]
    fn the_company_wants_a_paired_device_and_takes_orders_only_by_post() {
        let req = |text: &str| http::parse_head(text.as_bytes()).unwrap().0;
        let get = http::route(&req("GET /api/company HTTP/1.1\r\n\r\n"));
        let post = http::route(&req("POST /api/company HTTP/1.1\r\n\r\n"));
        assert_eq!((get.clone(), post.clone()), (Route::Company, Route::CompanyOrder));
        assert!(get.needs_key() && post.needs_key());
        assert_eq!(http::route(&req("PUT /api/company HTTP/1.1\r\n\r\n")), Route::Method);
        // (the body is strict JSON of one order's shape: anything else is refused before the
        // company is even read)
        assert_eq!(super::order(b"{\"do\":\"build\",\"area\":\"wash\",\"path\":\"C:/x\"}").0, 400);
        assert_eq!(super::order(b"not json").0, 400);
    }
}
