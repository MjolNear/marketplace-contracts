use crate::*;
use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::AccountId;
use near_sdk::json_types::{U128, U64};

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SiteMetadata {
    pub name: String,
    pub nft_link: String,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct ApprovedNFT {
    pub contract_id: AccountId,
    pub token_id: TokenId,
    pub owner_id: AccountId,
    pub title: String,
    pub description: Option<String>,
    pub copies: U64,
    pub media_url: String,
    pub reference_url: String,
    pub mint_site: SiteMetadata,
    pub price: U128,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketArgs {
    pub json_nft: ApprovedNFT,
}

#[derive(Serialize, Deserialize, BorshSerialize, BorshDeserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct ApprovedCollection {
    pub contract_id: AccountId,
    pub title: String,
    pub desc: String,
    pub media: String,
    pub reference: Option<String>
}