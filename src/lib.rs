use std::cmp::{max, min};
use std::collections::HashMap;

use near_contract_standards::non_fungible_token::{hash_account_id, TokenId};
use near_sdk::{AccountId, Balance, BorshStorageKey, CryptoHash, env, Gas, near_bindgen, Promise, promise_result_as_success, PromiseResult, serde_json::json};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{UnorderedMap, UnorderedSet, Vector};
use near_sdk::ext_contract;
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::to_string;

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

    fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    );
}

#[ext_contract(ext_self)]
trait ExtSelf {
    fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        nft_uid: TokenUID,
        price: U128,
    );

    fn resolve_purchase_no_payouts(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        nft_uid: TokenUID,
        price: U128,
    );
}

const GAS_FOR_NFT_TRANSFER: Gas = Gas(20_000_000_000_000);
const BASE_GAS: Gas = Gas(5_000_000_000_000);
const GAS_FOR_ROYALTIES: Gas = Gas(BASE_GAS.0 * 10u64);
const NO_DEPOSIT: Balance = 0;

const TREASURY_FEE: u128 = 200;
// 0.02
const TREASURY_ID: &str = "treasury1.near";
const CONTRACT_ID: &str = "mjol.near";
const REMOVER_ACCOUNT_ID: &str = "cleaner.mjol.near";

const UID_DELIMITER: &str = ":";

pub type Payout = HashMap<AccountId, U128>;
pub type TokenUID = String;


#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Listings,
    TokenUIDToData,
    TokenUIDsByOwner,
    TokenUIDsByOwnerInner { account_id_hash: CryptoHash },
    ListingsSet,
    TokenUIDsByOwnerSet,
    TokenUIDsByOwnerInnerSet { account_id_hash: CryptoHash },
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PayoutStruct {
    pub payout: Payout,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct CollectionMetadata {
    pub collection_name: String,
    pub collection_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct ApprovedNFT {
    pub title: String,
    pub description: Option<String>,
    pub copies: U64,
    pub media_url: Option<String>,
    pub reference_url: Option<String>,
    pub collection_metadata: Option<CollectionMetadata>,
    pub price: U128,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct ApprovedNFTFull {
    pub contract_id: AccountId,
    pub token_id: TokenId,
    pub owner_id: AccountId,
    pub title: String,
    pub description: Option<String>,
    pub copies: U64,
    pub media_url: Option<String>,
    pub reference_url: Option<String>,
    pub collection_metadata: Option<CollectionMetadata>,
    pub price: U128,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketArgs {
    pub json_nft: ApprovedNFT,
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

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Contract {
    listings_old: Vector<TokenUID>,
    user_to_uids_old: UnorderedMap<AccountId, Vector<TokenUID>>,
    listings: UnorderedSet<TokenUID>,
    uid_to_data: UnorderedMap<TokenUID, TokenData>,
    user_to_uids: UnorderedMap<AccountId, UnorderedSet<TokenUID>>,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketData {
    pub tokens: Vec<TokenData>,
    pub has_next_batch: bool,
    pub total_count: u64,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            listings_old: Vector::new(StorageKey::Listings),
            user_to_uids_old: UnorderedMap::new(StorageKey::TokenUIDsByOwner),
            listings: UnorderedSet::new(StorageKey::ListingsSet),
            uid_to_data: UnorderedMap::new(StorageKey::TokenUIDToData),
            user_to_uids: UnorderedMap::new(StorageKey::TokenUIDsByOwnerSet),
        }
    }
}

#[near_bindgen]
impl Contract {
    #[init(ignore_state)]
    pub fn new() -> Self {
        assert_eq!(env::predecessor_account_id().to_string(), CONTRACT_ID);
        Self {
            listings_old: Vector::new(StorageKey::Listings),
            user_to_uids_old: UnorderedMap::new(StorageKey::TokenUIDsByOwner),
            listings: UnorderedSet::new(StorageKey::ListingsSet),
            uid_to_data: UnorderedMap::new(StorageKey::TokenUIDToData),
            user_to_uids: UnorderedMap::new(StorageKey::TokenUIDsByOwnerSet),
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
        assert_eq!(env::signer_account_id(), owner_id, "You are not the owner of the NFT");

        let nft_contract_id = env::predecessor_account_id();
        assert_ne!(env::signer_account_id(), nft_contract_id, "Cross contract call awaited");

        let MarketArgs {
            json_nft
        } = near_sdk::serde_json::from_str(&msg).expect("Not valid MarketArgs");

        let price = json_nft.price;


        let new_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);

        assert!(self.uid_to_data.get(&new_uid.clone()).is_none(),
                "This NFT is already on the market");

        // update users listing info
        let mut cur_users_token_uids = self
            .user_to_uids
            .get(&owner_id.clone())
            .unwrap_or_else(|| {
                UnorderedSet::new(StorageKey::TokenUIDsByOwnerInnerSet {
                    account_id_hash: hash_account_id(&owner_id.clone())
                }
                )
            });
        cur_users_token_uids.insert(&new_uid.clone());
        self
            .user_to_uids
            .insert(&owner_id.clone(), &cur_users_token_uids);

        // add new listing to all listings
        self.listings.insert(&new_uid.clone());

        // add new uid -> TokenData
        self.uid_to_data.insert(&new_uid.clone(), &TokenData {
            owner_id: owner_id.clone(),
            nft_contract_id: nft_contract_id.clone(),
            token_id: token_id.clone(),
            price: price.0,
            approval_id: approval_id.clone(),
        });

        let full_json_nft = ApprovedNFTFull {
            contract_id: nft_contract_id.clone(),
            token_id: token_id.clone(),
            owner_id: owner_id.clone(),
            title: json_nft.title,
            description: json_nft.description,
            copies: json_nft.copies,
            media_url: json_nft.media_url,
            reference_url: json_nft.reference_url,
            collection_metadata: json_nft.collection_metadata,
            price: json_nft.price,
        };

        env::log_str(&json!({
        "type": "nft_on_approve",
        "data": {
            "nft_contract_id": nft_contract_id,
            "token_id": token_id,
            "approval_id": U64::from(approval_id),
            "json_nft": to_string(&full_json_nft).unwrap()
        }
        }).to_string());
    }

    #[payable]
    #[private]
    pub fn verify_contract(&mut self,
                           contract_id: AccountId,
                           contract_name: String,
    ) {
        env::log_str(&json!({
            "type": "verify_contract",
            "data": {
                "contract_id": contract_id.clone(),
                "contract_name": contract_name.clone()
            }
        }).to_string())
    }

    #[payable]
    pub fn update_token_price(
        &mut self,
        nft_contract_id: AccountId,
        token_id: TokenId,
        price: U128,
    ) {
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        let nft_data = self.uid_to_data.get(&nft_uid.clone())
            .expect("NFT does not exist.");

        let owner_id = nft_data.owner_id.clone();
        let caller_id = env::predecessor_account_id();

        assert_eq!(owner_id, caller_id, "You are not the owner of the NFT");

        self.uid_to_data.insert(&nft_uid.clone(), &TokenData {
            price: price.0,
            ..nft_data
        });

        env::log_str(&json!({
            "type": "update_token_price",
            "data": {
                "nft_contract_id": nft_contract_id,
                "token_id": token_id,
                "owner_id": owner_id,
                "price": price
            }
        }).to_string());
    }

    #[payable]
    pub fn remove_from_market(
        &mut self,
        nft_contract_id: AccountId,
        token_id: TokenId,
    ) {
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        let nft_data = self.uid_to_data.get(&nft_uid.clone())
            .expect("NFT does not exist.");

        let owner_id = nft_data.owner_id.clone();
        let caller_id = env::predecessor_account_id();

        assert_eq!(owner_id, caller_id);

        self.remove_nft(owner_id, nft_uid);

        env::log_str(&json!({
            "type": "remove_from_market",
            "data": {
                "nft_contract_id": nft_contract_id,
                "token_id": token_id,
                "owner_id": nft_data.owner_id,
                "approval_id": U64::from(nft_data.approval_id),
                "price": U128::from(nft_data.price)
            }
        }).to_string());
    }

    #[payable]
    pub fn buy(
        &mut self,
        nft_contract_id: AccountId,
        token_id: TokenId,
        is_payouts_supported: bool,
    ) {
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        let nft_data = self.uid_to_data.get(&nft_uid.clone())
            .expect("NFT does not exist.");

        let cur_approval_id: u64 = nft_data.approval_id.clone();
        let cur_price: U128 = U128::from(nft_data.price.clone());
        let seller_id = nft_data.owner_id.clone();
        let buyer_id = env::predecessor_account_id();

        assert_eq!(U128::from(env::attached_deposit()), cur_price);
        assert_ne!(seller_id, buyer_id);

        if is_payouts_supported {
            nft_contract::nft_transfer_payout(
                buyer_id.clone(),      // receiver_id: ValidAccountId,
                token_id.clone(),      // token_id: TokenId,
                Some(cur_approval_id), // approval_id: Option<u64>,
                Some(cur_price),       // balance: Option<U128>,
                Some(10u32),           // max_len_payout: Option<u32>
                nft_contract_id.clone(),
                1,
                GAS_FOR_NFT_TRANSFER,
            ).then(ext_self::resolve_purchase(
                buyer_id,
                seller_id,
                nft_uid,
                cur_price,
                env::current_account_id(),
                NO_DEPOSIT,
                GAS_FOR_ROYALTIES,
            ));
        } else {
            nft_contract::nft_transfer(
                buyer_id.clone(),      // receiver_id: ValidAccountId,
                token_id.clone(),      // token_id: TokenId,
                Some(cur_approval_id), // approval_id: Option<u64>
                None,
                nft_contract_id.clone(),
                1,
                GAS_FOR_NFT_TRANSFER,
            ).then(ext_self::resolve_purchase_no_payouts(
                buyer_id,
                seller_id,
                nft_uid,
                cur_price,
                env::current_account_id(),
                NO_DEPOSIT,
                GAS_FOR_ROYALTIES,
            ));
        }
    }

    #[payable]
    pub fn remove_old_listing(&mut self, token_uid: TokenUID) {
        assert_eq!(env::predecessor_account_id().to_string(), REMOVER_ACCOUNT_ID);
        let token_data = self.uid_to_data.get(&token_uid.clone());

        if let Some(data) = token_data {
            self.remove_nft(data.owner_id.clone(), token_uid.clone());

            env::log_str(&json!({
            "type": "remove_old_listing",
            "data": {
                "nft_contract_id": data.nft_contract_id.clone(),
                "token_id": data.token_id.clone(),
                "owner_id": data.owner_id.clone(),
                "approval_id": U64::from(data.approval_id.clone()),
                "price": U128::from(data.price.clone())
            }
        }).to_string());
        } else {
            env::panic_str("Token is not on the market.")
        }
    }

    pub fn get_nfts(self, from: u64, limit: u64) -> MarketData {
        let size = self.listings.len() as u64;
        let mut res = vec![];
        if from >= size {
            return MarketData {
                tokens: res,
                has_next_batch: false,
                total_count: size,
            };
        }
        let real_to = (size - from) as usize;
        let real_from = max(real_to as i64 - limit as i64, 0 as i64) as usize;

        for i in (real_from..real_to).rev() {
            res.push(self.uid_to_data
                .get(&self.listings.as_vector().get(i as u64).unwrap()).unwrap())
        }

        MarketData {
            tokens: res,
            has_next_batch: real_from > 0,
            total_count: size,
        }
    }

    pub fn get_user_nfts(self, owner_id: AccountId) -> Vec<TokenData> {
        let all_uids = self.user_to_uids
            .get(&owner_id.clone());

        return if let Some(uids) = all_uids {
            uids.iter().map(|x| {
                self.uid_to_data.get(&x.clone()).unwrap()
            }).collect()
        } else {
            vec![]
        };
    }

    pub fn get_nft_price(self, token_uid: TokenUID) -> Option<u128> {
        let token = self.uid_to_data.get(&token_uid);
        if let Some(token) = token {
            return Some(token.price);
        }
        None
    }

    #[private]
    pub fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        nft_uid: TokenUID,
        price: U128,
    ) {
        assert_eq!(env::promise_results_count(), 1);
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Failed => env::panic_str("NFT Transfer failed. Try again."),
            PromiseResult::Successful(_) => ()
        }

        self.remove_nft(seller_id.clone(), nft_uid.clone());

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

            let mut log_payout = Payout::new();
            log_payout.insert(seller_id.clone(), U128::from(price.0 - treasury_fee));

            env::log_str(
                &json!({
                    "type": "resolve_purchase",
                    "data": {
                        "price": U128::from(price),
                        "buyer_id": buyer_id,
                        "seller_id": seller_id,
                        "nft_uid": nft_uid,
                        "payout": log_payout
                    }
                }).to_string()
            );
            return;
        };

        // 2% fee for treasury
        let treasury_fee = price.0 * TREASURY_FEE / 10_000u128;

        for (receiver_id, amount) in payout.clone() {
            if receiver_id == seller_id {
                Promise::new(receiver_id).transfer(amount.0 - treasury_fee);
                Promise::new(AccountId::new_unchecked(TREASURY_ID.to_string())).transfer(treasury_fee);
            } else {
                Promise::new(receiver_id).transfer(amount.0);
            }
        }

        let mut log_payout = payout.clone();
        *log_payout
            .get_mut(&seller_id.clone()).unwrap() = U128::from(
            log_payout[&seller_id.clone()].0 - treasury_fee
        );
        env::log_str(
            &json!({
                    "type": "resolve_purchase",
                    "data": {
                        "price": U128::from(price),
                        "buyer_id": buyer_id,
                        "seller_id": seller_id,
                        "nft_uid": nft_uid,
                        "payout": log_payout
                    }
                }).to_string()
        );
    }

    #[private]
    pub fn resolve_purchase_no_payouts(&mut self, buyer_id: AccountId, seller_id: AccountId,
                                       nft_uid: TokenUID, price: U128) {
        assert_eq!(env::promise_results_count(), 1);
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Failed => env::panic_str("NFT Transfer failed. Try again."),
            PromiseResult::Successful(_) => ()
        }

        self.remove_nft(seller_id.clone(), nft_uid.clone());

        let treasury_fee = price.0 * TREASURY_FEE / 10_000u128;
        Promise::new(seller_id.clone())
            .transfer(price.0 - treasury_fee);
        Promise::new(AccountId::new_unchecked(TREASURY_ID.to_string()))
            .transfer(treasury_fee);

        let mut log_payout = Payout::new();
        log_payout.insert(seller_id.clone(), U128::from(price.0 - treasury_fee));

        env::log_str(
            &json!({
                    "type": "resolve_purchase",
                    "data": {
                        "price": U128::from(price),
                        "buyer_id": buyer_id,
                        "seller_id": seller_id,
                        "nft_uid": nft_uid,
                        "payout": log_payout
                    }
                }).to_string()
        );
    }

    // #[init(ignore_state)]
    // #[private]
    // pub fn migrate() -> Self {
    //     #[derive(BorshDeserialize)]
    //     struct Old {
    //         listings: Vector<TokenUID>,
    //         uid_to_data: UnorderedMap<TokenUID, TokenData>,
    //         user_to_uids: UnorderedMap<AccountId, Vector<TokenUID>>,
    //     }
    //
    //     let prev_state: Old = env::state_read().expect("No such state.");
    //
    //     let mut new_listings: UnorderedSet<TokenUID> = UnorderedSet::new(StorageKey::ListingsSet);
    //     let mut new_user_to_uids: UnorderedMap<AccountId, UnorderedSet<TokenUID>> =
    //         UnorderedMap::new(StorageKey::TokenUIDsByOwnerSet);
    //
    //     for (acc, uids) in prev_state.user_to_uids.iter() {
    //         let mut new_uids = new_user_to_uids
    //             .get(&acc.clone())
    //             .unwrap_or_else(|| {
    //                 UnorderedSet::new(StorageKey::TokenUIDsByOwnerInnerSet {
    //                     account_id_hash: hash_account_id(&acc.clone())
    //                 }
    //                 )
    //             });
    //         for uid in uids.iter() {
    //             new_uids.insert(&uid.clone());
    //         }
    //
    //         new_user_to_uids.insert(&acc.clone(), &new_uids);
    //     }
    //
    //     for listing in prev_state.listings.iter() {
    //         new_listings.insert(&listing.clone());
    //     }
    //
    //     Self {
    //         listings: new_listings,
    //         uid_to_data: prev_state.uid_to_data,
    //         user_to_uids: new_user_to_uids,
    //     }
    // }

    #[init(ignore_state)]
    #[private]
    pub fn migrate_start() -> Self {
        #[derive(BorshDeserialize)]
        struct Old {
            listings: Vector<TokenUID>,
            uid_to_data: UnorderedMap<TokenUID, TokenData>,
            user_to_uids: UnorderedMap<AccountId, Vector<TokenUID>>,
        }

        let prev_state: Old = env::state_read().expect("No such state.");

        Self {
            listings: UnorderedSet::new(StorageKey::ListingsSet),
            uid_to_data: prev_state.uid_to_data,
            user_to_uids: UnorderedMap::new(StorageKey::TokenUIDsByOwnerSet),
            listings_old: prev_state.listings,
            user_to_uids_old: prev_state.user_to_uids,
        }
    }

    #[init(ignore_state)]
    #[private]
    pub fn migrate_users_uids(from_user: u64, user_bs: usize, from_user_listing: u64, listing_bs: usize) -> Self {
        #[derive(BorshDeserialize)]
        struct Old {
            listings_old: Vector<TokenUID>,
            user_to_uids_old: UnorderedMap<AccountId, Vector<TokenUID>>,
            listings: UnorderedSet<TokenUID>,
            uid_to_data: UnorderedMap<TokenUID, TokenData>,
            user_to_uids: UnorderedMap<AccountId, UnorderedSet<TokenUID>>,
        }

        let mut prev_state: Old = env::state_read().expect("No such state.");

        let users_to = min((from_user as usize) + user_bs, prev_state.user_to_uids_old.len() as usize);
        for (acc, uids) in
        &prev_state.user_to_uids_old.to_vec()[(from_user as usize)..users_to] {
            let mut new_uids = prev_state.user_to_uids
                .get(&acc.clone())
                .unwrap_or_else(|| {
                    UnorderedSet::new(StorageKey::TokenUIDsByOwnerInnerSet {
                        account_id_hash: hash_account_id(&acc.clone())
                    }
                    )
                });
            env::log_str(&format!("{} -> {}/{}",
                                  acc.clone(),
                                  min(from_user_listing + listing_bs as u64, uids.len()),
                                  uids.len()));
            let uids_to = min((from_user_listing as usize) + listing_bs, uids.len() as usize);
            for uid in &uids.to_vec()[(from_user_listing as usize)..uids_to] {
                new_uids.insert(&uid.clone());
            }

            prev_state.user_to_uids.insert(&acc.clone(), &new_uids);
        }

        Self {
            listings: prev_state.listings,
            uid_to_data: prev_state.uid_to_data,
            user_to_uids: prev_state.user_to_uids,
            user_to_uids_old: prev_state.user_to_uids_old,
            listings_old: prev_state.listings_old,
        }
    }

    #[init(ignore_state)]
    #[private]
    pub fn migrate_listings(from: u64, bs: usize) -> Self {
        #[derive(BorshDeserialize)]
        struct Old {
            listings_old: Vector<TokenUID>,
            user_to_uids_old: UnorderedMap<AccountId, Vector<TokenUID>>,
            listings: UnorderedSet<TokenUID>,
            uid_to_data: UnorderedMap<TokenUID, TokenData>,
            user_to_uids: UnorderedMap<AccountId, UnorderedSet<TokenUID>>,
        }

        let mut prev_state: Old = env::state_read().expect("No such state.");

        env::log_str(&format!("LISTINGS -> {}/{}",
                              min(from + bs as u64, prev_state.listings_old.len()),
                              prev_state.listings_old.len()));

        let listings_to = min((from as usize) + bs, prev_state.listings_old.len() as usize);
        for listing in &prev_state.listings_old.to_vec()[(from as usize)..listings_to] {
            prev_state.listings.insert(&listing.clone());
        }

        Self {
            listings: prev_state.listings,
            uid_to_data: prev_state.uid_to_data,
            user_to_uids: prev_state.user_to_uids,
            user_to_uids_old: prev_state.user_to_uids_old,
            listings_old: prev_state.listings_old,
        }
    }

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

    fn remove_nft(&mut self, owner_id: AccountId, nft_uid: TokenUID) {
        // delete from owner's listings
        let mut cur_users_token_uids = self
            .user_to_uids
            .get(&owner_id.clone())
            .unwrap_or_else(|| {
                UnorderedSet::new(StorageKey::TokenUIDsByOwnerInnerSet {
                    account_id_hash: hash_account_id(&owner_id.clone())
                }
                )
            });
        assert!(cur_users_token_uids.remove(&nft_uid.clone()));
        self
            .user_to_uids
            .insert(&owner_id.clone(), &cur_users_token_uids);

        // delete from all listings
        assert!(self.listings.remove(&nft_uid.clone()));

        // delete info about NFT
        assert!(self.uid_to_data.remove(&nft_uid.clone()).is_some());
    }
}
