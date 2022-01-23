use crate::*;
use near_sdk::{AccountId, Balance};
use crate::TokenUID;

pub type OfferId = String;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct Offer {
    pub token_uid: TokenUID,
    pub offer_id: OfferId,
    pub price: Balance,
    pub buyer_id: AccountId,
}