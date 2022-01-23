use crate::*;
use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::{AccountId, Balance};

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenData {
    pub owner_id: AccountId,
    pub nft_contract_id: AccountId,
    pub token_id: TokenId,
    pub price: Balance,
    pub approval_id: u64,
}