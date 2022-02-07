use std::collections::VecDeque;
use near_sdk::collections::Vector;
use crate::{Offer, OfferId, TokenUID};

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

pub fn delete_from_vector_by_offer_id(offers: &mut Vector<Offer>, offer_id: &OfferId) -> Option<Offer> {
    let mut save = VecDeque::new();
    while offers.len() > 0
        && offer_id.to_string() != offers.get(offers.len() - 1).unwrap().offer_id {
        save.push_front(offers.pop().unwrap());
    }
    if offers.len() > 0 {
        let to_remove = offers.pop().unwrap();
        offers.extend(save);
        Some(to_remove)
    } else {
        offers.extend(save);
        None
    }
}