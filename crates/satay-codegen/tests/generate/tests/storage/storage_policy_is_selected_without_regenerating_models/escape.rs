//! Must not compile: the decoded model borrows from the arena it was built in.

use crate::generated::Child;
use crate::policies::Arena;

pub fn escape() {
    let child = {
        let arena = Arena::default();
        satay_runtime::from_json_slice_in::<Child<'_, Arena>, _>(br#"{"label":"kid"}"#, &arena)
            .unwrap()
    };
    drop(child);
}
