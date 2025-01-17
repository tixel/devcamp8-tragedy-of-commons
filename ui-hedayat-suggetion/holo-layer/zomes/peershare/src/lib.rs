use crate::helpers::progenitor::DnaProperties;
use hdk::prelude::*;
mod entries;
mod helpers;
use entries::schema::{Schema, SchemaDTO};
use helpers::utils::*;
use holo_hash::AgentPubKeyB64;
use holo_hash::EntryHashB64;
/***** Entry Definition */

entry_defs![Schema::entry_def()];

#[hdk_extern]
pub fn who_am_i_2(_: ()) -> ExternResult<AgentPubKeyB64> {
    Ok(AgentPubKeyB64::from(agent_info()?.agent_latest_pubkey))
}

#[hdk_extern]
pub fn who_am_i(_: ()) -> ExternResult<AgentPubKey> {
    Ok(agent_info()?.agent_latest_pubkey)
}

#[hdk_extern]
pub fn get_dna_props(_: ()) -> ExternResult<DnaProperties> {
    helpers::progenitor::DnaProperties::get()
}

#[hdk_extern]
pub fn am_i_developer(_: ()) -> ExternResult<bool> {
    helpers::progenitor::am_i_developer()
}

/***** Schema */
#[hdk_extern]
pub fn create_schema(input: SchemaDTO) -> ExternResult<EntryHashB64> {
    if false == helpers::progenitor::am_i_developer()? {
        return Err(err(
            "You are not the developer, so you can't create a schema",
        ));
    }
    return entries::schema::create_schema(input);
}

#[hdk_extern]
pub fn get_schema_element(input: SchemaDTO) -> ExternResult<Element> {
    let hash: EntryHash = hash_entry(&Schema::new(&input.definition, &input.version))?;
    let element: Element =
        get(EntryHash::from(hash), GetOptions::default())?.ok_or(err("Can't find this schema"))?;

    Ok(element)
}
