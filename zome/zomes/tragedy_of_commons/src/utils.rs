use hdk::prelude::*;
use holo_hash::AgentPubKeyB64;

use crate::game_move::GameMove;
// NOTE: didn't had time to figure out how to apply this once on a lib level
// TODO: remove it later

pub fn try_get_and_convert<T: 'static + TryFrom<Entry>>(entry_hash: EntryHash) -> ExternResult<T> {
    match get(entry_hash.clone(), GetOptions::default())? {
        Some(element) => try_from_element(element),
        None => Err(crate::err("Entry not found")),
    }
}

pub fn try_get_by_header_and_convert<T: 'static + TryFrom<Entry>>(header_hash: HeaderHash) -> ExternResult<T> {
    match get(header_hash.clone(), GetOptions::default())? {
        Some(element) => try_from_element(element),
        None => Err(crate::err("Entry not found")),
    }
}

pub fn try_get_game_moves(entry_hash: EntryHash) -> Vec<GameMove> {
    let result = get_links(entry_hash, Some(LinkTag::new("game_move")));
    let links = result.unwrap();
    let mut items: Vec<GameMove> = vec![];
    for link in links.into_inner() {
        let item: GameMove = try_get_and_convert(link.target).unwrap();
        items.push(item)
    }
    items
}

pub fn try_from_element<T: TryFrom<Entry>>(element: Element) -> ExternResult<T> {
    match element.entry() {
        element::ElementEntry::Present(entry) => {
            T::try_from(entry.clone()).or(Err(crate::err("Cannot convert entry")))
        }
        _ => Err(crate::err("Could not convert element")),
    }
}

pub fn entry_hash_from_element(element:Element) -> ExternResult<&'static EntryHash> {
    let hash = element.header().entry_hash();
    match hash {
        Some(e) => Ok(e),
        None => Err(WasmError::Guest("cannot extract entry from element".into())),
    }
}
/// Converts binary string pub keys into binary array pub keys.
/// Binary string format is used for sending data to UI,
/// and binary array format is used for working with keys on the backend
/// TODO(e-nastasia): I think it may make sense to keep agent pub keys as binary arrays
/// and only convert to binary string when sending data to UI?
pub fn convert_keys_from_b64(input: Vec<AgentPubKeyB64>) -> Vec<AgentPubKey> {
    input.iter().map(|k| AgentPubKey::from(k.clone())).collect()
}
