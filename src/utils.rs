use std::collections::VecDeque;
use near_sdk::collections::Vector;
use crate::TokenUID;

pub fn delete_from_vector_by_uid(uids: &mut Vector<TokenUID>, uid: &TokenUID) -> Option<TokenUID> {
    let mut save = VecDeque::new();
    while uids.len() > 0 && uid.to_string() != uids.get(uids.len() - 1).unwrap() {
        save.push_front(uids.pop().unwrap());
    }
    if uids.len() > 0 {
        let to_remove = uids.pop().unwrap();
        uids.extend(save);
        Some(to_remove)
    } else {
        uids.extend(save);
        None
    }
}