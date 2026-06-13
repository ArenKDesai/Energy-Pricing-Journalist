//! IndexedDB persistence for the rolling 3-day window of snapshots.
//!
//! Because the only CORS-accessible MISO endpoint is the live snapshot, history
//! is accumulated forward and persisted locally so it survives reloads and
//! grows toward the full 3-day window over time. Snapshots are keyed by their
//! interval timestamp, so re-fetching an interval overwrites it in place.

use std::cell::RefCell;
use std::rc::Rc;

use futures::channel::oneshot;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    Event, IdbDatabase, IdbKeyRange, IdbObjectStore, IdbOpenDbRequest, IdbRequest,
    IdbTransactionMode,
};

use crate::types::Snapshot;

const DB_NAME: &str = "miso_lmp";
const STORE: &str = "snapshots";

/// Resolve an `IdbRequest` to its result, awaiting onsuccess/onerror once.
async fn await_request(req: &IdbRequest) -> Result<JsValue, JsValue> {
    let (tx, rx) = oneshot::channel::<Result<JsValue, JsValue>>();
    let tx = Rc::new(RefCell::new(Some(tx)));

    let req_s = req.clone();
    let tx_s = tx.clone();
    let onsuccess = Closure::wrap(Box::new(move |_e: Event| {
        if let Some(tx) = tx_s.borrow_mut().take() {
            let _ = tx.send(Ok(req_s.result().unwrap_or(JsValue::UNDEFINED)));
        }
    }) as Box<dyn FnMut(Event)>);

    let tx_e = tx.clone();
    let onerror = Closure::wrap(Box::new(move |_e: Event| {
        if let Some(tx) = tx_e.borrow_mut().take() {
            let _ = tx.send(Err(JsValue::from_str("IndexedDB request failed")));
        }
    }) as Box<dyn FnMut(Event)>);

    req.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
    req.set_onerror(Some(onerror.as_ref().unchecked_ref()));

    let result = rx.await.unwrap_or(Err(JsValue::from_str("request dropped")));
    drop(onsuccess);
    drop(onerror);
    result
}

/// Open (and if needed create) the database and object store.
pub async fn open_db() -> Result<IdbDatabase, JsValue> {
    let factory = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .indexed_db()?
        .ok_or_else(|| JsValue::from_str("IndexedDB unavailable"))?;

    let open_req: IdbOpenDbRequest = factory.open_with_u32(DB_NAME, 1)?;

    // Create the object store on first open / version upgrade.
    let onupgrade = Closure::wrap(Box::new(move |e: Event| {
        if let Some(req) = e.target().and_then(|t| t.dyn_into::<IdbOpenDbRequest>().ok()) {
            if let Ok(result) = req.result() {
                if let Ok(db) = result.dyn_into::<IdbDatabase>() {
                    if !db.object_store_names().contains(STORE) {
                        let _ = db.create_object_store(STORE);
                    }
                }
            }
        }
    }) as Box<dyn FnMut(Event)>);
    open_req.set_onupgradeneeded(Some(onupgrade.as_ref().unchecked_ref()));

    let result = await_request(open_req.as_ref()).await?;
    drop(onupgrade);
    result.dyn_into::<IdbDatabase>()
}

fn store(db: &IdbDatabase, mode: IdbTransactionMode) -> Result<IdbObjectStore, JsValue> {
    let tx = db.transaction_with_str_and_mode(STORE, mode)?;
    tx.object_store(STORE)
}

/// Persist (insert or overwrite) a snapshot, keyed by its interval timestamp.
pub async fn put_snapshot(db: &IdbDatabase, snap: &Snapshot) -> Result<(), JsValue> {
    let st = store(db, IdbTransactionMode::Readwrite)?;
    let value = serde_wasm_bindgen::to_value(snap)?;
    let key = JsValue::from_f64(snap.ts as f64);
    let req = st.put_with_key(&value, &key)?;
    await_request(&req).await?;
    Ok(())
}

/// Load every stored snapshot, sorted ascending by timestamp.
pub async fn load_all(db: &IdbDatabase) -> Result<Vec<Snapshot>, JsValue> {
    let st = store(db, IdbTransactionMode::Readonly)?;
    let req = st.get_all()?;
    let result = await_request(&req).await?;

    let arr = js_sys::Array::from(&result);
    let mut out: Vec<Snapshot> = Vec::with_capacity(arr.length() as usize);
    for v in arr.iter() {
        if let Ok(snap) = serde_wasm_bindgen::from_value::<Snapshot>(v) {
            out.push(snap);
        }
    }
    out.sort_by_key(|s| s.ts);
    Ok(out)
}

/// Delete all snapshots older than `cutoff_ts` (epoch seconds).
pub async fn prune_before(db: &IdbDatabase, cutoff_ts: i64) -> Result<(), JsValue> {
    let st = store(db, IdbTransactionMode::Readwrite)?;
    // Keys strictly below the cutoff.
    let range = IdbKeyRange::upper_bound_with_open(&JsValue::from_f64(cutoff_ts as f64), true)?;
    let req = st.delete(&range)?;
    await_request(&req).await?;
    Ok(())
}
