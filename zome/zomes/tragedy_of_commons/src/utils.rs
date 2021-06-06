// NOTE: didn't had time to figure out how to apply this once on a lib level
// TODO: remove it later

#[allow(dead_code)]
use hdk::prelude::*;

pub fn try_get_and_convert<T: 'static + TryFrom<Entry>>(entry_hash: EntryHash) -> ExternResult<T> {
    match get(entry_hash.clone(), GetOptions::default())? {
        Some(element) => try_from_element(element),
        None => Err(crate::err("Entry not found")),
    }
}

pub fn try_from_element<T: TryFrom<Entry>>(element: Element) -> ExternResult<T> {
    match element.entry() {
        element::ElementEntry::Present(entry) => {
            T::try_from(entry.clone()).or(Err(crate::err("Cannot convert entry")))
        }
        _ => Err(crate::err("Could not convert element")),
    }
}
