//! Code that never chooses a storage policy. It must compile without naming a
//! lifetime, a policy, or a storage trait, and without any turbofish.
#![deny(rust_2018_idioms)]

use std::collections::BTreeMap;

use crate::generated::{
    Cat, Child, Grid, Label, Pet, Record, RecordChoice, RecordState, StoreRecordInput,
    StoreRecordResponse,
};

pub type Saved = Record;

pub struct Cache {
    pub last: Option<Record>,
    pub grids: Vec<Grid>,
}

pub fn record(name: Label) -> Record {
    Record {
        name,
        labels: BTreeMap::from([("team".to_owned(), "ops".to_owned())]),
        state: RecordState::Other("future".to_owned()),
        children: vec![Child {
            label: "kid".to_owned(),
        }],
        choice: Some(RecordChoice::Integer(7)),
        pet: Some(Pet::Cat(Cat {
            name: "Tom".to_owned(),
            lives: Some(9),
        })),
    }
}

pub fn describe(record: &Record) -> String {
    format!("{record:?}")
}

pub fn input(record: Record) -> StoreRecordInput {
    StoreRecordInput::new("a/b", record).region("west")
}

pub fn stored(response: StoreRecordResponse) -> Option<Saved> {
    match response {
        StoreRecordResponse::Ok(record) => Some(record),
        StoreRecordResponse::UnexpectedStatus(..) => None,
    }
}
