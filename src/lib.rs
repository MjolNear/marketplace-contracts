mod utils;

use std::cmp::min;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, near_bindgen, Gas, promise_result_as_success, serde_json::json, AccountId, Promise, Balance, CryptoHash, BorshStorageKey};
use near_sdk::collections::{LookupMap, Vector};
use std::collections::HashMap;
use near_sdk::serde::{Deserialize, Serialize};

use near_sdk::ext_contract;
use near_contract_standards::non_fungible_token::{hash_account_id, TokenId};
use near_sdk::json_types::U128;
use crate::utils::delete_from_vector_by_uid;


#[ext_contract(nft_contract)]
trait ExtContract {
    fn nft_transfer_payout(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        balance: Option<U128>,
        max_len_payout: Option<u32>,
    );
}

#[ext_contract(ext_self)]
trait ExtSelf {
    fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        price: U128,
    ) -> Promise;
}

const GAS_FOR_NFT_TRANSFER: Gas = Gas(20_000_000_000_000);
const BASE_GAS: Gas = Gas(5_000_000_000_000);
const GAS_FOR_ROYALTIES: Gas = Gas(BASE_GAS.0 * 10u64);
const NO_DEPOSIT: Balance = 0;

const TREASURY_FEE: u128 = 200;
// 0.02
const TREASURY_ID: &str = "kekmemlol.testnet";

const UID_DELIMITER: &str = ":";

pub type Payout = HashMap<AccountId, U128>;
pub type TokenUID = String;


#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Listings,
    TokenUIDToData,
    TokenUIDsByOwner,
    TokenUIDsByOwnerInner { account_id_hash: CryptoHash },
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PayoutStruct {
    pub payout: Payout,
}


#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Contract {
    listings: Vector<TokenUID>,
    uid_to_data: LookupMap<TokenUID, TokenData>,
    user_to_uids: LookupMap<AccountId, Vector<TokenUID>>,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketArgs {
    pub price: U128,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenData {
    pub owner_id: AccountId,
    pub nft_contract_id: AccountId,
    pub token_id: TokenId,
    pub price: u128,
    pub approval_id: u64,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            listings: Vector::new(StorageKey::Listings),
            uid_to_data: LookupMap::new(StorageKey::TokenUIDToData),
            user_to_uids: LookupMap::new(StorageKey::TokenUIDsByOwner),
        }
    }
}

#[near_bindgen]
impl Contract {
    #[init(ignore_state)]
    pub fn new() -> Self {
        Self {
            listings: Vector::new(StorageKey::Listings),
            uid_to_data: LookupMap::new(StorageKey::TokenUIDToData),
            user_to_uids: LookupMap::new(StorageKey::TokenUIDsByOwner),
        }
    }

    #[payable]
    pub fn nft_on_approve(
        &mut self,
        token_id: TokenId,
        owner_id: AccountId,
        approval_id: u64,
        msg: String,
    ) {
        assert_eq!(env::signer_account_id(), owner_id, "You are not the owner of NFT");

        let nft_contract_id = env::predecessor_account_id();
        assert_ne!(env::signer_account_id(), nft_contract_id, "Cross contract call awaited");

        let MarketArgs {
            price
        } = near_sdk::serde_json::from_str(&msg).expect("Not valid MarketArgs");


        let new_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);

        // update users listing info
        let mut cur_users_token_uids = self
            .user_to_uids
            .get(&owner_id.clone())
            .unwrap_or_else(|| {
                Vector::new(StorageKey::TokenUIDsByOwnerInner {
                    account_id_hash: hash_account_id(&owner_id.clone())
                }
                )
            });
        cur_users_token_uids.push(&new_uid.clone());
        self
            .user_to_uids
            .insert(&owner_id.clone(), &cur_users_token_uids);

        // add new listing to all listings
        self.listings.push(&new_uid.clone());

        // add new uid -> TokenData
        self.uid_to_data.insert(&new_uid.clone(), &TokenData {
            owner_id: owner_id.clone(),
            nft_contract_id: nft_contract_id.clone(),
            token_id: token_id.clone(),
            price: price.0,
            approval_id: approval_id.clone(),
        });

        env::log_str(
            &*format!("Added token {} from contract {} to the market.",
                      token_id,
                      nft_contract_id)
        )
    }

//    #[payable]
//    pub fn buy(
//        &mut self,
//        nft_contract_id: AccountId,
//        token_id: TokenId,
//    ) {
//
//    }

    #[payable]
    pub fn buy_with_payouts(
        &mut self,
        nft_contract_id: AccountId,
        token_id: TokenId,
    ) {
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        let nft_data = self.uid_to_data.get(&nft_uid.clone())
            .expect("NFT does not exist.");

        let cur_approval_id: u64 = nft_data.approval_id;
        let cur_price: U128 = U128::from(nft_data.price);
        let seller_id = nft_data.owner_id;

        assert_eq!(U128::from(env::attached_deposit()), cur_price);

        // delete from owner's listings
        let mut cur_users_token_uids = self
            .user_to_uids
            .get(&seller_id.clone())
            .unwrap_or_else(|| {
                Vector::new(StorageKey::TokenUIDsByOwnerInner {
                    account_id_hash: hash_account_id(&seller_id.clone())
                }
                )
            });
        assert!(delete_from_vector_by_uid(&mut cur_users_token_uids, &nft_uid.clone()).is_some());
        self
            .user_to_uids
            .insert(&seller_id.clone(), &cur_users_token_uids);

        // delete from all listings
        assert!(delete_from_vector_by_uid(&mut self.listings, &nft_uid.clone()).is_some());

        // delete info about NFT
        assert!(self.uid_to_data.remove(&nft_uid.clone()).is_some());

        let buyer_id = env::signer_account_id();

        nft_contract::nft_transfer_payout(
            buyer_id.clone(),      // receiver_id: ValidAccountId,
            token_id,              // token_id: TokenId,
            Some(cur_approval_id), // approval_id: Option<u64>,
            Some(cur_price),       // balance: Option<U128>,
            Some(10u32),           // max_len_payout: Option<u32>
            nft_contract_id,
            1,
            GAS_FOR_NFT_TRANSFER,
        ).then(ext_self::resolve_purchase(
            buyer_id,
            seller_id,
            cur_price,
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_ROYALTIES,
        ));
    }

    #[private]
    fn check_payouts(
        price: U128,
        payout: Payout,
    ) -> Option<Payout> {
        let mut remainder = price.0;
        for &value in payout.values() {
            remainder = remainder.checked_sub(value.0)?;
        }
        if remainder == 0 || remainder == 1 {
            Some(payout)
        } else {
            None
        }
    }

    #[private]
    pub fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        price: U128,
    ) {
        let payout_option = promise_result_as_success().and_then(|value| {

            // If Payout is struct with payout field than get it
            let res = near_sdk::serde_json::from_slice::<PayoutStruct>(&value);
            if res.is_ok() {
                res.ok().and_then(|payout| {
                    Contract::check_payouts(price, payout.payout)
                })
            } else {
                near_sdk::serde_json::from_slice::<Payout>(&value).ok().and_then(|payout| {
                    Contract::check_payouts(price, payout)
                })
            }
        });

        let payout = if let Some(payout_option) = payout_option {
            payout_option
        } else {
            let treasury_fee = price.0 * TREASURY_FEE / 10_000u128;
            Promise::new(seller_id.clone())
                .transfer(price.0 - treasury_fee);
            Promise::new(AccountId::new_unchecked(TREASURY_ID.to_string()))
                .transfer(treasury_fee);

            env::log_str(
                &json!({
                    "type": "resolve_purchase_fail",
                    "params": {
                        "price": price,
                        "buyer_id": buyer_id
                    }
                }).to_string()
            );
            return;
        };

        // 2% fee for treasury
        let treasury_fee = price.0 * TREASURY_FEE / 10_000u128;

        for (receiver_id, amount) in payout {
            if receiver_id == seller_id {
                Promise::new(receiver_id).transfer(amount.0 - treasury_fee);
                Promise::new(AccountId::new_unchecked(TREASURY_ID.to_string())).transfer(treasury_fee);
            } else {
                Promise::new(receiver_id).transfer(amount.0);
            }
        }
        env::log_str(
            &json!({
                "type": "resolve_purchase",
                "params": {
                    "price": price,
                    "buyer_id": buyer_id,
                }
            }).to_string()
        );
    }

    pub fn get_nfts(self, from: u64, limit: u64) -> Vec<TokenData> {
        let size = self.listings.len() as u64;
        if from >= size {
            return vec![];
        }
        let real_from = (size - from - 1) as usize;
        let real_to = min(real_from + limit, size as usize);

        let mut res = vec![];
        for i in real_from..real_to {
            res.push(self.uid_to_data
                .get(&self.listings.get(i as u64).unwrap()).unwrap())
        }
        res
    }

    pub fn get_user_nfts(self, owner_id: AccountId) -> Vec<(TokenUID, u128)> {
        let all_uids = self.user_to_uids
            .get(&owner_id.clone());
        if let Some(uids) = all_uids {
            uids.iter().map(|x| {
                (x.clone(), self.uid_to_data.get(&x.clone()).unwrap().price)
            })
                .collect()
        } else {
            return vec![];
        }
    }
}